//! Stability AI: sixteen models across the thirteen v2beta endpoints.
//!
//! Every submit is a `multipart/form-data` POST with `Accept: image/*` and
//! Bearer auth. Fourteen services answer the submit with raw image bytes; the
//! two async services answer `{"id": "<64 hex>"}` and poll
//! `GET /v2beta/results/{id}`, which returns the image bytes directly on 200.

use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::Duration;

use base64::Engine;
use serde_json::{Value, json};

use crate::error::{FieldError, ProviderError};
use crate::http::{Body, HttpHeader, HttpRequest, HttpResponse, Method, Part};
use crate::job::{EncodedImage, JobRequest, PollPolicy};
use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::providers::bfl::protocol::extract_result_mime;
use crate::providers::{ImageInput, InlineImage, JobId, PollOutcome, ReferenceStep, Submitted};
use crate::scene::Scene;
use crate::schema::{FieldDef, FieldKind, ParamSchema, RuleContext};

const ACCEPT: &str = "image/*";
const MAX_BODY_BYTES: u64 = 100 * 1024 * 1024;
const RATIO_MIN: f64 = 1.0 / 2.5;
const RATIO_MAX: f64 = 2.5;

const ASPECT_RATIOS: &[&str] = &[
    "21:9", "16:9", "3:2", "5:4", "1:1", "4:5", "2:3", "9:16", "9:21",
];
const STYLE_PRESETS: &[&str] = &[
    "enhance",
    "anime",
    "photographic",
    "digital-art",
    "comic-book",
    "fantasy-art",
    "line-art",
    "analog-film",
    "neon-punk",
    "isometric",
    "low-poly",
    "origami",
    "modeling-compound",
    "cinematic",
    "3d-model",
    "pixel-art",
    "tile-texture",
];
const LIGHT_DIRECTIONS: &[&str] = &["left", "right", "above", "below"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prompt {
    Required,
    Optional,
    Unused,
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    None,
    Switch,
    Sd35,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ratio {
    None,
    All,
    FirstImage,
}

/// Documented input image bounds for one service.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    min_side: u32,
    max_side: Option<u32>,
    min_pixels: u64,
    max_pixels: Option<u64>,
    ratio: Ratio,
}

impl Bounds {
    fn check(&self, index: usize, width: u32, height: u32) -> Vec<FieldError> {
        let mut errors = Vec::new();
        let mut push = |message: String| {
            errors.push(FieldError {
                key: "images".into(),
                message,
                approved_max: None,
            });
        };
        if width < self.min_side || height < self.min_side {
            push(format!(
                "input image must be at least {} pixels on each side",
                self.min_side
            ));
        }
        if let Some(max_side) = self.max_side
            && (width > max_side || height > max_side)
        {
            push(format!(
                "input image must be at most {max_side} pixels on each side"
            ));
        }
        let pixels = u64::from(width) * u64::from(height);
        if pixels < self.min_pixels {
            push(format!(
                "input image must be at least {} pixels",
                self.min_pixels
            ));
        }
        if let Some(max_pixels) = self.max_pixels
            && pixels > max_pixels
        {
            push(format!("input image must be at most {max_pixels} pixels"));
        }
        let ratio_applies = match self.ratio {
            Ratio::None => false,
            Ratio::All => true,
            Ratio::FirstImage => index == 0,
        };
        if ratio_applies {
            let ratio = f64::from(width) / f64::from(height);
            if !(RATIO_MIN..=RATIO_MAX).contains(&ratio) {
                push("input image aspect ratio must be between 1:2.5 and 2.5:1".into());
            }
        }
        errors
    }
}

/// Everything the shared builder and validator need to know about one service.
#[derive(Debug)]
struct Descriptor {
    spec: ModelSpec,
    prompt: Prompt,
    parts: &'static [&'static str],
    mode: Mode,
    minimum_images: usize,
    async_submit: bool,
    outpaint: bool,
    background: bool,
    bounds: Option<Bounds>,
}

#[derive(Debug, Clone, Copy)]
pub struct StableImageUltra;

#[derive(Debug, Clone, Copy)]
pub struct StableImageCore;

#[derive(Debug, Clone, Copy)]
pub struct Sd35Large;

#[derive(Debug, Clone, Copy)]
pub struct Sd35LargeTurbo;

#[derive(Debug, Clone, Copy)]
pub struct Sd35Medium;

#[derive(Debug, Clone, Copy)]
pub struct Sd35Flash;

#[derive(Debug, Clone, Copy)]
pub struct UpscaleFast;

#[derive(Debug, Clone, Copy)]
pub struct UpscaleConservative;

#[derive(Debug, Clone, Copy)]
pub struct UpscaleCreative;

#[derive(Debug, Clone, Copy)]
pub struct Outpaint;

#[derive(Debug, Clone, Copy)]
pub struct Erase;

#[derive(Debug, Clone, Copy)]
pub struct Inpaint;

#[derive(Debug, Clone, Copy)]
pub struct SearchAndReplace;

#[derive(Debug, Clone, Copy)]
pub struct SearchAndRecolor;

#[derive(Debug, Clone, Copy)]
pub struct RemoveBackground;

#[derive(Debug, Clone, Copy)]
pub struct ReplaceBackgroundAndRelight;

fn spec(id: &str, name: &str, path: &str, slots: ImageSlots, fields: Vec<FieldDef>) -> ModelSpec {
    ModelSpec {
        id: id.into(),
        display_name: name.into(),
        provider: ProviderId::Stability,
        auth: AuthScheme::Bearer,
        endpoint: format!("{}{path}", ProviderId::Stability.base_url()),
        beta: false,
        capabilities: Capabilities {
            supports_boxes: false,
            image_slots: slots,
            size_control: SizeControl::AspectRatioAndResolution,
        },
        schema: ParamSchema {
            fields,
            cross_rules: Vec::new(),
        },
    }
}

fn descriptor(
    id: &'static str,
    name: &'static str,
    path: &'static str,
    slots: ImageSlots,
    prompt: Prompt,
    parts: &'static [&'static str],
    mode: Mode,
    minimum_images: usize,
    bounds: Option<Bounds>,
    fields: Vec<FieldDef>,
) -> Descriptor {
    Descriptor {
        spec: spec(id, name, path, slots, fields),
        prompt,
        parts,
        mode,
        minimum_images,
        async_submit: false,
        outpaint: false,
        background: false,
        bounds,
    }
}

fn field(
    key: &str,
    label: &str,
    help: Option<&str>,
    kind: FieldKind,
    required: bool,
    default: Option<Value>,
) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: help.map(std::borrow::ToOwned::to_owned),
        kind,
        required,
        default,
        locked_when: None,
    }
}

fn select(key: &str, label: &str, options: &[&str], default: Option<&str>) -> FieldDef {
    field(
        key,
        label,
        None,
        FieldKind::Select {
            options: options.iter().map(|option| (*option).to_owned()).collect(),
        },
        false,
        default.map(|value| json!(value)),
    )
}

fn text(key: &str, label: &str) -> FieldDef {
    field(
        key,
        label,
        None,
        FieldKind::Text { multiline: false },
        false,
        None,
    )
}

fn required_text(key: &str, label: &str) -> FieldDef {
    field(
        key,
        label,
        None,
        FieldKind::Text { multiline: false },
        true,
        None,
    )
}

fn int(
    key: &str,
    label: &str,
    help: Option<&str>,
    min: i64,
    max: i64,
    default: Option<i64>,
) -> FieldDef {
    field(
        key,
        label,
        help,
        FieldKind::Int { min, max, step: 1 },
        false,
        default.map(|value| json!(value)),
    )
}

fn float(key: &str, label: &str, min: f64, max: f64, default: f64) -> FieldDef {
    field(
        key,
        label,
        None,
        FieldKind::Float {
            min,
            max,
            step: 0.05,
        },
        false,
        Some(json!(default)),
    )
}

fn bool(key: &str, label: &str, default: bool) -> FieldDef {
    field(
        key,
        label,
        None,
        FieldKind::Bool,
        false,
        Some(json!(default)),
    )
}

fn aspect_ratio() -> FieldDef {
    select("aspect_ratio", "Aspect ratio", ASPECT_RATIOS, Some("1:1"))
}

fn output_format(options: &[&str]) -> FieldDef {
    select("output_format", "Output format", options, Some("png"))
}

fn style_preset() -> FieldDef {
    select("style_preset", "Style preset", STYLE_PRESETS, None)
}

fn negative_prompt() -> FieldDef {
    text("negative_prompt", "Negative prompt")
}

fn seed() -> FieldDef {
    int("seed", "Seed", None, 0, 4_294_967_294, None)
}

fn strength(default: f64) -> FieldDef {
    float("strength", "Strength", 0.0, 1.0, default)
}

fn cfg_scale(default: f64) -> FieldDef {
    float("cfg_scale", "CFG scale", 1.0, 10.0, default)
}

fn creativity(min: f64, max: f64, default: f64) -> FieldDef {
    float("creativity", "Creativity", min, max, default)
}

fn ultra_fields() -> Vec<FieldDef> {
    vec![
        aspect_ratio(),
        strength(0.7),
        negative_prompt(),
        seed(),
        style_preset(),
        output_format(&["png", "jpeg", "webp"]),
    ]
}

fn core_fields() -> Vec<FieldDef> {
    vec![
        aspect_ratio(),
        negative_prompt(),
        seed(),
        style_preset(),
        output_format(&["png", "jpeg", "webp"]),
    ]
}

fn sd35_fields(cfg_default: f64, strength_default: f64) -> Vec<FieldDef> {
    vec![
        aspect_ratio(),
        strength(strength_default),
        cfg_scale(cfg_default),
        negative_prompt(),
        seed(),
        style_preset(),
        output_format(&["png", "jpeg", "webp"]),
    ]
}

fn grow_mask(max: i64, default: i64) -> FieldDef {
    int(
        "grow_mask",
        "Grow mask",
        Some("The second loaded image is sent as the mask."),
        0,
        max,
        Some(default),
    )
}

fn replace_background_fields() -> Vec<FieldDef> {
    vec![
        text("foreground_prompt", "Foreground prompt"),
        negative_prompt(),
        float(
            "preserve_original_subject",
            "Preserve original subject",
            0.0,
            1.0,
            0.6,
        ),
        float(
            "original_background_depth",
            "Original background depth",
            0.0,
            1.0,
            0.5,
        ),
        bool(
            "keep_original_background",
            "Keep original background",
            false,
        ),
        field(
            "light_source_direction",
            "Light source direction",
            Some("A third loaded image is sent as the light reference."),
            FieldKind::Select {
                options: LIGHT_DIRECTIONS
                    .iter()
                    .map(|option| (*option).to_owned())
                    .collect(),
            },
            false,
            None,
        ),
        float(
            "light_source_strength",
            "Light source strength",
            0.0,
            1.0,
            0.3,
        ),
        seed(),
        output_format(&["png", "jpeg", "webp"]),
    ]
}

const ULTRA_BOUNDS: Bounds = Bounds {
    min_side: 64,
    max_side: Some(16_384),
    min_pixels: 4_096,
    max_pixels: None,
    ratio: Ratio::None,
};

const SD35_BOUNDS: Bounds = Bounds {
    min_side: 64,
    max_side: None,
    min_pixels: 0,
    max_pixels: None,
    ratio: Ratio::None,
};

const FAST_BOUNDS: Bounds = Bounds {
    min_side: 32,
    max_side: Some(1_536),
    min_pixels: 1_024,
    max_pixels: Some(1_048_576),
    ratio: Ratio::None,
};

const CONSERVATIVE_BOUNDS: Bounds = Bounds {
    min_side: 64,
    max_side: None,
    min_pixels: 4_096,
    max_pixels: Some(9_437_184),
    ratio: Ratio::All,
};

const CREATIVE_BOUNDS: Bounds = Bounds {
    min_side: 64,
    max_side: None,
    min_pixels: 4_096,
    max_pixels: Some(1_048_576),
    ratio: Ratio::None,
};

const PLAIN_EDIT_BOUNDS: Bounds = Bounds {
    min_side: 64,
    max_side: None,
    min_pixels: 4_096,
    max_pixels: Some(9_437_184),
    ratio: Ratio::None,
};

const RATIO_EDIT_BOUNDS: Bounds = Bounds {
    min_side: 64,
    max_side: None,
    min_pixels: 4_096,
    max_pixels: Some(9_437_184),
    ratio: Ratio::All,
};

const REMOVE_BACKGROUND_BOUNDS: Bounds = Bounds {
    min_side: 64,
    max_side: None,
    min_pixels: 4_096,
    max_pixels: Some(4_194_304),
    ratio: Ratio::None,
};

const REPLACE_BACKGROUND_BOUNDS: Bounds = Bounds {
    min_side: 64,
    max_side: None,
    min_pixels: 4_096,
    max_pixels: Some(9_437_184),
    ratio: Ratio::FirstImage,
};

static ULTRA: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "stability-ultra",
        "Stable Image Ultra",
        "/v2beta/stable-image/generate/ultra",
        ImageSlots::Single,
        Prompt::Required,
        &["image"],
        Mode::Switch,
        0,
        Some(ULTRA_BOUNDS),
        ultra_fields(),
    )
});

static CORE: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "stability-core",
        "Stable Image Core",
        "/v2beta/stable-image/generate/core",
        ImageSlots::None,
        Prompt::Required,
        &[],
        Mode::None,
        0,
        None,
        core_fields(),
    )
});

static SD35_LARGE: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "sd3.5-large",
        "SD 3.5 Large",
        "/v2beta/stable-image/generate/sd3",
        ImageSlots::Single,
        Prompt::Required,
        &["image"],
        Mode::Sd35,
        0,
        Some(SD35_BOUNDS),
        sd35_fields(4.0, 0.7),
    )
});

static SD35_LARGE_TURBO: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "sd3.5-large-turbo",
        "SD 3.5 Large Turbo",
        "/v2beta/stable-image/generate/sd3",
        ImageSlots::Single,
        Prompt::Required,
        &["image"],
        Mode::Sd35,
        0,
        Some(SD35_BOUNDS),
        sd35_fields(1.0, 0.7),
    )
});

static SD35_MEDIUM: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "sd3.5-medium",
        "SD 3.5 Medium",
        "/v2beta/stable-image/generate/sd3",
        ImageSlots::Single,
        Prompt::Required,
        &["image"],
        Mode::Sd35,
        0,
        Some(SD35_BOUNDS),
        sd35_fields(4.0, 0.7),
    )
});

static SD35_FLASH: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "sd3.5-flash",
        "SD 3.5 Flash",
        "/v2beta/stable-image/generate/sd3",
        ImageSlots::Single,
        Prompt::Required,
        &["image"],
        Mode::Sd35,
        0,
        Some(SD35_BOUNDS),
        sd35_fields(1.0, 0.95),
    )
});

static UPSCALE_FAST: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "stability-upscale-fast",
        "Fast Upscale",
        "/v2beta/stable-image/upscale/fast",
        ImageSlots::Single,
        Prompt::Unused,
        &["image"],
        Mode::None,
        1,
        Some(FAST_BOUNDS),
        vec![output_format(&["png", "jpeg", "webp"])],
    )
});

static UPSCALE_CONSERVATIVE: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "stability-upscale-conservative",
        "Conservative Upscale",
        "/v2beta/stable-image/upscale/conservative",
        ImageSlots::Single,
        Prompt::Required,
        &["image"],
        Mode::None,
        1,
        Some(CONSERVATIVE_BOUNDS),
        vec![
            creativity(0.2, 0.5, 0.35),
            negative_prompt(),
            seed(),
            output_format(&["png", "jpeg", "webp"]),
        ],
    )
});

static UPSCALE_CREATIVE: LazyLock<Descriptor> = LazyLock::new(|| Descriptor {
    async_submit: true,
    ..descriptor(
        "stability-upscale-creative",
        "Creative Upscale",
        "/v2beta/stable-image/upscale/creative",
        ImageSlots::Single,
        Prompt::Required,
        &["image"],
        Mode::None,
        1,
        Some(CREATIVE_BOUNDS),
        vec![
            creativity(0.1, 0.5, 0.3),
            negative_prompt(),
            seed(),
            style_preset(),
            output_format(&["png", "jpeg", "webp"]),
        ],
    )
});

static OUTPAINT: LazyLock<Descriptor> = LazyLock::new(|| Descriptor {
    outpaint: true,
    ..descriptor(
        "stability-outpaint",
        "Outpaint",
        "/v2beta/stable-image/edit/outpaint",
        ImageSlots::Single,
        Prompt::Optional,
        &["image"],
        Mode::None,
        1,
        Some(RATIO_EDIT_BOUNDS),
        vec![
            int("left", "Left", None, 0, 2000, Some(0)),
            int("right", "Right", None, 0, 2000, Some(0)),
            int("up", "Up", None, 0, 2000, Some(0)),
            int("down", "Down", None, 0, 2000, Some(0)),
            creativity(0.0, 1.0, 0.5),
            seed(),
            style_preset(),
            output_format(&["png", "jpeg", "webp"]),
        ],
    )
});

static ERASE: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "stability-erase",
        "Erase",
        "/v2beta/stable-image/edit/erase",
        ImageSlots::List {
            max: 2,
            key: "image".into(),
        },
        Prompt::Unused,
        &["image", "mask"],
        Mode::None,
        1,
        Some(PLAIN_EDIT_BOUNDS),
        vec![
            grow_mask(20, 5),
            seed(),
            output_format(&["png", "jpeg", "webp"]),
        ],
    )
});

static INPAINT: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "stability-inpaint",
        "Inpaint",
        "/v2beta/stable-image/edit/inpaint",
        ImageSlots::List {
            max: 2,
            key: "image".into(),
        },
        Prompt::Required,
        &["image", "mask"],
        Mode::None,
        1,
        Some(PLAIN_EDIT_BOUNDS),
        vec![
            grow_mask(100, 5),
            negative_prompt(),
            seed(),
            style_preset(),
            output_format(&["png", "jpeg", "webp"]),
        ],
    )
});

static SEARCH_AND_REPLACE: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "stability-search-and-replace",
        "Search and Replace",
        "/v2beta/stable-image/edit/search-and-replace",
        ImageSlots::Single,
        Prompt::Required,
        &["image"],
        Mode::None,
        1,
        Some(RATIO_EDIT_BOUNDS),
        vec![
            required_text("search_prompt", "Search prompt"),
            grow_mask(20, 3),
            negative_prompt(),
            seed(),
            style_preset(),
            output_format(&["png", "jpeg", "webp"]),
        ],
    )
});

static SEARCH_AND_RECOLOR: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "stability-search-and-recolor",
        "Search and Recolor",
        "/v2beta/stable-image/edit/search-and-recolor",
        ImageSlots::Single,
        Prompt::Required,
        &["image"],
        Mode::None,
        1,
        Some(RATIO_EDIT_BOUNDS),
        vec![
            required_text("select_prompt", "Select prompt"),
            grow_mask(20, 3),
            negative_prompt(),
            seed(),
            style_preset(),
            output_format(&["png", "jpeg", "webp"]),
        ],
    )
});

static REMOVE_BACKGROUND: LazyLock<Descriptor> = LazyLock::new(|| {
    descriptor(
        "stability-remove-background",
        "Remove Background",
        "/v2beta/stable-image/edit/remove-background",
        ImageSlots::Single,
        Prompt::Unused,
        &["image"],
        Mode::None,
        1,
        Some(REMOVE_BACKGROUND_BOUNDS),
        vec![output_format(&["png", "webp"])],
    )
});

static REPLACE_BACKGROUND_AND_RELIGHT: LazyLock<Descriptor> = LazyLock::new(|| Descriptor {
    async_submit: true,
    background: true,
    ..descriptor(
        "stability-replace-background-and-relight",
        "Replace Background and Relight",
        "/v2beta/stable-image/edit/replace-background-and-relight",
        ImageSlots::List {
            max: 3,
            key: "subject_image".into(),
        },
        Prompt::Background,
        &["subject_image", "background_reference", "light_reference"],
        Mode::None,
        1,
        Some(REPLACE_BACKGROUND_BOUNDS),
        replace_background_fields(),
    )
});

macro_rules! stability_model {
    ($ty:ident, $descriptor:ident) => {
        impl $ty {
            #[must_use]
            pub fn model_spec() -> &'static ModelSpec {
                &$descriptor.spec
            }
        }

        impl crate::providers::Provider for $ty {
            fn spec(&self) -> &ModelSpec {
                &$descriptor.spec
            }

            fn validate_for_submit(
                &self,
                scene: &Scene,
                values: &BTreeMap<String, Value>,
                image_count: usize,
            ) -> Vec<FieldError> {
                validate(&$descriptor, scene, values, image_count)
            }

            fn reference_steps(&self, _images: &[EncodedImage]) -> Vec<ReferenceStep> {
                Vec::new()
            }

            fn build_submit(&self, request: &JobRequest, _images: &[ImageInput]) -> HttpRequest {
                build_submit(&$descriptor, request)
            }

            fn parse_submit(&self, response: &HttpResponse) -> Result<Submitted, ProviderError> {
                if $descriptor.async_submit {
                    parse_submit_async(response)
                } else {
                    match classify(response) {
                        Ok(_) => Err(ProviderError::Protocol(
                            "stability returns the image on the submit response".into(),
                        )),
                        Err(error) => Err(error),
                    }
                }
            }

            fn inline_image(&self, response: &HttpResponse) -> Option<InlineImage> {
                classify(response).ok()
            }

            fn parse_poll(&self, response: &HttpResponse) -> PollOutcome {
                parse_poll(response)
            }

            fn poll_request(&self, submitted: &Submitted) -> HttpRequest {
                poll_request(submitted)
            }

            fn poll_policy(&self) -> PollPolicy {
                poll_policy()
            }

            fn prep_pixel_cap(&self) -> u64 {
                $descriptor
                    .bounds
                    .and_then(|bounds| bounds.max_pixels)
                    .unwrap_or(9_437_184)
            }
        }
    };
}

stability_model!(StableImageUltra, ULTRA);
stability_model!(StableImageCore, CORE);
stability_model!(Sd35Large, SD35_LARGE);
stability_model!(Sd35LargeTurbo, SD35_LARGE_TURBO);
stability_model!(Sd35Medium, SD35_MEDIUM);
stability_model!(Sd35Flash, SD35_FLASH);
stability_model!(UpscaleFast, UPSCALE_FAST);
stability_model!(UpscaleConservative, UPSCALE_CONSERVATIVE);
stability_model!(UpscaleCreative, UPSCALE_CREATIVE);
stability_model!(Outpaint, OUTPAINT);
stability_model!(Erase, ERASE);
stability_model!(Inpaint, INPAINT);
stability_model!(SearchAndReplace, SEARCH_AND_REPLACE);
stability_model!(SearchAndRecolor, SEARCH_AND_RECOLOR);
stability_model!(RemoveBackground, REMOVE_BACKGROUND);
stability_model!(ReplaceBackgroundAndRelight, REPLACE_BACKGROUND_AND_RELIGHT);

fn validate(
    descriptor: &Descriptor,
    scene: &Scene,
    values: &BTreeMap<String, Value>,
    image_count: usize,
) -> Vec<FieldError> {
    let ctx = RuleContext {
        has_source_image: scene.source_image.is_some(),
        image_count,
        box_count: scene.boxes.len(),
    };
    let mut errors = descriptor.spec.schema.validate(values, &ctx);
    let prose_empty = scene.prose.trim().is_empty();
    let prompt_missing = match descriptor.prompt {
        Prompt::Required => prose_empty,
        Prompt::Background => image_count < 2 && prose_empty,
        Prompt::Optional | Prompt::Unused => false,
    };
    if prompt_missing {
        errors.push(FieldError {
            key: "prompt".into(),
            message: "Prompt is empty".into(),
            approved_max: None,
        });
    }
    if descriptor.minimum_images > 0 && image_count == 0 {
        errors.push(FieldError {
            key: "images".into(),
            message: "Add at least one image.".into(),
            approved_max: None,
        });
    }
    if descriptor.outpaint {
        let sum: i64 = ["left", "right", "up", "down"]
            .into_iter()
            .map(|key| values.get(key).and_then(Value::as_i64).unwrap_or(0))
            .sum();
        if sum <= 0 {
            errors.push(FieldError {
                key: "outpaint".into(),
                message: "Set at least one outpaint direction above 0.".into(),
                approved_max: None,
            });
        }
    }
    if let Some(bounds) = descriptor.bounds {
        for (index, slot) in scene.slots().into_iter().take(image_count).enumerate() {
            if slot.width == 0 || slot.height == 0 {
                continue;
            }
            errors.extend(bounds.check(index, slot.width, slot.height));
        }
    }
    errors
}

fn build_submit(descriptor: &Descriptor, request: &JobRequest) -> HttpRequest {
    let mut parts: Vec<Part> = Vec::new();
    let prose = request.compiled_prompt.as_str();
    let prose_empty = prose.trim().is_empty();
    let has_image = !request.images.is_empty();
    let mode_switch = descriptor.mode != Mode::None;

    match descriptor.prompt {
        Prompt::Required => push_text(&mut parts, "prompt", prose),
        Prompt::Optional => {
            if !prose_empty {
                push_text(&mut parts, "prompt", prose);
            }
        }
        Prompt::Unused => {}
        Prompt::Background => {
            if !prose_empty {
                push_text(&mut parts, "background_prompt", prose);
            }
        }
    }

    if descriptor.mode == Mode::Sd35 {
        push_text(
            &mut parts,
            "mode",
            if has_image {
                "image-to-image"
            } else {
                "text-to-image"
            },
        );
        push_text(&mut parts, "model", &descriptor.spec.id);
    }

    for (name, image) in descriptor.parts.iter().zip(request.images.iter()) {
        parts.push(binary_part(name, image));
    }

    let light_source_set = request.images.len() >= 3
        || request
            .params
            .get("light_source_direction")
            .and_then(Value::as_str)
            .is_some_and(|direction| !direction.is_empty());

    for field in &descriptor.spec.schema.fields {
        if mode_switch && field.key == "aspect_ratio" && has_image {
            continue;
        }
        if mode_switch && field.key == "strength" && !has_image {
            continue;
        }
        if descriptor.background && field.key == "light_source_strength" && !light_source_set {
            continue;
        }
        let value = match request
            .params
            .get(&field.key)
            .filter(|value| !value.is_null())
        {
            Some(value) => Some(value),
            None => field
                .default
                .as_ref()
                .filter(|_| mode_switch && field.key == "strength" && has_image),
        };
        let Some(value) = value else {
            continue;
        };
        match &field.kind {
            FieldKind::Text { .. } => {
                let Some(text) = value.as_str() else { continue };
                if text.trim().is_empty() {
                    continue;
                }
                push_text(&mut parts, &field.key, text);
            }
            FieldKind::Select { .. } => {
                let Some(text) = value.as_str() else { continue };
                if text.is_empty() {
                    continue;
                }
                push_text(&mut parts, &field.key, text);
            }
            FieldKind::Int { .. } => {
                let Some(number) = value.as_i64() else {
                    continue;
                };
                if field.key == "seed" && number == 0 {
                    continue;
                }
                push_text(&mut parts, &field.key, &number.to_string());
            }
            FieldKind::Float { .. } => {
                let Some(number) = value.as_f64() else {
                    continue;
                };
                push_text(&mut parts, &field.key, &number_text(number));
            }
            FieldKind::Bool => {
                let on = value.as_bool().unwrap_or(false);
                push_text(&mut parts, &field.key, if on { "true" } else { "false" });
            }
            _ => {}
        }
    }

    HttpRequest {
        method: Method::Post,
        url: descriptor.spec.endpoint.clone(),
        headers: vec![HttpHeader {
            name: "Accept".into(),
            value: ACCEPT.into(),
            sensitive: false,
        }],
        auth: Some(ProviderId::Stability),
        body: Body::Multipart { parts },
        max_body_bytes: MAX_BODY_BYTES,
        redact_query: false,
    }
}

fn push_text(parts: &mut Vec<Part>, name: &str, value: &str) {
    parts.push(Part {
        name: name.into(),
        filename: None,
        content_type: "text/plain".into(),
        bytes: value.as_bytes().to_vec(),
    });
}

fn binary_part(name: &str, image: &EncodedImage) -> Part {
    let extension = match image.media_type.as_str() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        _ => "png",
    };
    Part {
        name: name.into(),
        filename: Some(format!("image.{extension}")),
        content_type: image.media_type.clone(),
        bytes: image.bytes.clone(),
    }
}

fn number_text(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        (number as i64).to_string()
    } else {
        format!("{number}")
    }
}

fn poll_request(submitted: &Submitted) -> HttpRequest {
    HttpRequest {
        method: Method::Get,
        url: submitted.poll_url.to_string(),
        headers: vec![HttpHeader {
            name: "Accept".into(),
            value: ACCEPT.into(),
            sensitive: false,
        }],
        auth: Some(ProviderId::Stability),
        body: Body::Empty,
        max_body_bytes: MAX_BODY_BYTES,
        redact_query: false,
    }
}

fn poll_policy() -> PollPolicy {
    PollPolicy {
        initial: Duration::from_secs(10),
        factor: 1.0,
        cap: Duration::from_secs(10),
        deadline: Duration::from_secs(600),
    }
}

fn classify(response: &HttpResponse) -> Result<InlineImage, ProviderError> {
    let json = body_json(response);
    if response.status == 403 || blocked(response, json.as_ref()) {
        return Err(ProviderError::Blocked);
    }
    if response.status != 200 {
        return Err(status_error(response, false));
    }
    image_from_parts(response, json.as_ref()).ok_or(ProviderError::NoImage)
}

fn parse_submit_async(response: &HttpResponse) -> Result<Submitted, ProviderError> {
    let json = serde_json::from_slice::<Value>(&response.body).ok();
    if response.status == 403 || blocked(response, json.as_ref()) {
        return Err(ProviderError::Blocked);
    }
    if response.status != 200 {
        return Err(status_error(response, false));
    }
    let value =
        json.ok_or_else(|| ProviderError::Protocol("submit response was not json".into()))?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::Protocol("submit response had no id".into()))?;
    let job_id = JobId::parse(id).map_err(ProviderError::Protocol)?;
    let poll_url = url::Url::parse(&format!(
        "{}/v2beta/results/{}",
        ProviderId::Stability.base_url(),
        job_id.as_str()
    ))
    .map_err(|_| ProviderError::Protocol("poll url was not valid".into()))?;
    Ok(Submitted {
        job_id,
        poll_url,
        quote: None,
    })
}

fn parse_poll(response: &HttpResponse) -> PollOutcome {
    let json = body_json(response);
    if response.status == 403 || blocked(response, json.as_ref()) {
        return PollOutcome::Failed(ProviderError::Blocked);
    }
    match response.status {
        202 => PollOutcome::InFlight {
            status: "in-progress".into(),
            progress: None,
        },
        200 => match image_from_parts(response, json.as_ref()) {
            Some(_) => PollOutcome::Failed(ProviderError::Protocol(
                "poll returned the image inline".into(),
            )),
            None => PollOutcome::Failed(ProviderError::NoImage),
        },
        _ => PollOutcome::Failed(status_error(response, true)),
    }
}

fn body_json(response: &HttpResponse) -> Option<Value> {
    let first = response
        .body
        .iter()
        .find(|byte| !byte.is_ascii_whitespace())?;
    if *first != b'{' {
        return None;
    }
    serde_json::from_slice(&response.body).ok()
}

fn blocked(response: &HttpResponse, json: Option<&Value>) -> bool {
    response.headers.iter().any(|header| {
        header.name.eq_ignore_ascii_case("finish-reason")
            && header.value.trim().eq_ignore_ascii_case("CONTENT_FILTERED")
    }) || json.is_some_and(|value| {
        value
            .get("finish_reason")
            .and_then(Value::as_str)
            .is_some_and(|reason| reason.eq_ignore_ascii_case("CONTENT_FILTERED"))
            || value
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name == "content_moderation")
    })
}

fn image_from_parts(response: &HttpResponse, json: Option<&Value>) -> Option<InlineImage> {
    if let Some(media) = header_media(response) {
        if response.body.is_empty() {
            return None;
        }
        let sniffed = extract_result_mime(None, &response.body)?;
        if sniffed != media {
            return None;
        }
        return Some(InlineImage {
            bytes: response.body.clone(),
            mime: media.to_string(),
            interaction_id: String::new(),
            suggestion: String::new(),
        });
    }
    image_from_json(json?)
}

fn image_from_json(value: &Value) -> Option<InlineImage> {
    let encoded = value.get("image")?.as_str()?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    if bytes.is_empty() {
        return None;
    }
    let mime = extract_result_mime(None, &bytes)?;
    Some(InlineImage {
        bytes,
        mime: mime.to_string(),
        interaction_id: String::new(),
        suggestion: String::new(),
    })
}

fn header_media(response: &HttpResponse) -> Option<&'static str> {
    let content_type = response
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"))?
        .value
        .as_str();
    let media = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match media.as_str() {
        "image/png" => Some("image/png"),
        "image/jpeg" | "image/jpg" => Some("image/jpeg"),
        "image/webp" => Some("image/webp"),
        _ => None,
    }
}

fn status_error(response: &HttpResponse, poll: bool) -> ProviderError {
    match response.status {
        401 => ProviderError::Auth,
        402 => ProviderError::OutOfCredits { hint: None },
        403 => ProviderError::Blocked,
        404 if poll => ProviderError::Protocol("generation not found or expired".into()),
        404 => ProviderError::ModelNotFound,
        429 => ProviderError::RateLimited { retry_after: None },
        400 | 413 | 422 => match joined_errors(&response.body) {
            Some(message) => ProviderError::Validation {
                message,
                field_errors: Vec::new(),
            },
            None => ProviderError::Http {
                status: response.status,
                body: lossy(&response.body),
            },
        },
        status => ProviderError::Http {
            status,
            body: lossy(&response.body),
        },
    }
}

fn joined_errors(body: &[u8]) -> Option<String> {
    let value = serde_json::from_slice::<Value>(body).ok()?;
    let errors = value.get("errors")?.as_array()?;
    let messages: Vec<&str> = errors.iter().filter_map(Value::as_str).collect();
    (!messages.is_empty()).then(|| messages.join("; "))
}

fn lossy(body: &[u8]) -> String {
    String::from_utf8_lossy(body).into_owned()
}

#[cfg(test)]
mod tests {
    use crate::providers::{ModelRegistry, Provider};
    use crate::scene::ImageRef;

    use super::*;
    use serde_json::json;

    const IDS: [&str; 16] = [
        "stability-ultra",
        "stability-core",
        "sd3.5-large",
        "sd3.5-large-turbo",
        "sd3.5-medium",
        "sd3.5-flash",
        "stability-upscale-fast",
        "stability-upscale-conservative",
        "stability-upscale-creative",
        "stability-outpaint",
        "stability-erase",
        "stability-inpaint",
        "stability-search-and-replace",
        "stability-search-and-recolor",
        "stability-remove-background",
        "stability-replace-background-and-relight",
    ];

    const NAMES: [&str; 16] = [
        "Stable Image Ultra",
        "Stable Image Core",
        "SD 3.5 Large",
        "SD 3.5 Large Turbo",
        "SD 3.5 Medium",
        "SD 3.5 Flash",
        "Fast Upscale",
        "Conservative Upscale",
        "Creative Upscale",
        "Outpaint",
        "Erase",
        "Inpaint",
        "Search and Replace",
        "Search and Recolor",
        "Remove Background",
        "Replace Background and Relight",
    ];

    const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\nimagebytes";
    const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
    const WEBP_BYTES: &[u8] = b"RIFF\x24\x00\x00\x00WEBPVP8L";

    fn provider(id: &str) -> Box<dyn Provider> {
        crate::providers::resolve_model(id).unwrap_or_else(|| panic!("missing {id}"))
    }

    fn spec_of(id: &str) -> &'static ModelSpec {
        ModelRegistry::specs()
            .iter()
            .copied()
            .find(|spec| spec.id == id)
            .unwrap_or_else(|| panic!("missing {id}"))
    }

    fn defaults(id: &str) -> BTreeMap<String, Value> {
        spec_of(id).schema.defaults()
    }

    fn encoded(media_type: &str, seed: u8) -> EncodedImage {
        EncodedImage {
            media_type: media_type.into(),
            bytes: vec![seed; 16],
            width: 512,
            height: 512,
            content_hash: format!("h{seed}"),
        }
    }

    fn request(
        model: &str,
        params: BTreeMap<String, Value>,
        images: Vec<EncodedImage>,
    ) -> JobRequest {
        request_with(model, "a lighthouse", params, images)
    }

    fn request_with(
        model: &str,
        prose: &str,
        params: BTreeMap<String, Value>,
        images: Vec<EncodedImage>,
    ) -> JobRequest {
        JobRequest {
            model_id: model.into(),
            compiled_prompt: prose.into(),
            params,
            images,
            box_snapshot: Vec::new(),
            created_at: "2026-10-25T00:00:00Z".into(),
            conversation: None,
        }
    }

    fn submit(id: &str, params: BTreeMap<String, Value>, images: Vec<EncodedImage>) -> HttpRequest {
        submit_with(id, "a lighthouse", params, images)
    }

    fn submit_with(
        id: &str,
        prose: &str,
        params: BTreeMap<String, Value>,
        images: Vec<EncodedImage>,
    ) -> HttpRequest {
        provider(id).build_submit(&request_with(id, prose, params, images), &[])
    }

    fn parts_of(http: &HttpRequest) -> &Vec<Part> {
        let Body::Multipart { parts } = &http.body else {
            panic!("expected multipart body");
        };
        parts
    }

    fn part_names(http: &HttpRequest) -> Vec<&str> {
        parts_of(http)
            .iter()
            .map(|part| part.name.as_str())
            .collect()
    }

    fn text_part<'a>(http: &'a HttpRequest, name: &str) -> &'a Part {
        parts_of(http)
            .iter()
            .find(|part| part.name == name)
            .unwrap_or_else(|| panic!("no `{name}` part"))
    }

    fn text_value(http: &HttpRequest, name: &str) -> String {
        let part = text_part(http, name);
        assert_eq!(part.filename, None, "{name}");
        assert_eq!(part.content_type, "text/plain", "{name}");
        String::from_utf8(part.bytes.clone()).unwrap()
    }

    fn binary_part<'a>(http: &'a HttpRequest, name: &str) -> &'a Part {
        parts_of(http)
            .iter()
            .find(|part| part.name == name)
            .unwrap_or_else(|| panic!("no `{name}` part"))
    }

    fn scene_with_prose(prose: &str) -> Scene {
        let mut scene = Scene::empty();
        scene.prose = prose.into();
        scene
    }

    fn scene_with_image(width: u32, height: u32) -> Scene {
        let mut scene = Scene::empty();
        scene.source_image = Some(ImageRef::new("canvas.png", width, height));
        scene
    }

    fn response(status: u16, content_type: Option<&str>, body: &[u8]) -> HttpResponse {
        let headers = content_type
            .map(|value| {
                vec![HttpHeader {
                    name: "Content-Type".into(),
                    value: value.into(),
                    sensitive: false,
                }]
            })
            .unwrap_or_default();
        HttpResponse {
            status,
            headers,
            body: body.to_vec(),
        }
    }

    fn header_response(
        status: u16,
        content_type: &str,
        body: &[u8],
        name: &str,
        value: &str,
    ) -> HttpResponse {
        HttpResponse {
            status,
            headers: vec![
                HttpHeader {
                    name: "Content-Type".into(),
                    value: content_type.into(),
                    sensitive: false,
                },
                HttpHeader {
                    name: name.into(),
                    value: value.into(),
                    sensitive: false,
                },
            ],
            body: body.to_vec(),
        }
    }

    #[test]
    fn registry_lists_the_sixteen_stability_models_in_menu_order() {
        let ids: Vec<_> = ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.provider == ProviderId::Stability)
            .map(|spec| spec.id.as_str())
            .collect();
        assert_eq!(ids, IDS);
        let names: Vec<_> = ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.provider == ProviderId::Stability)
            .map(|spec| spec.display_name.as_str())
            .collect();
        assert_eq!(names, NAMES);
        for id in IDS {
            let spec = spec_of(id);
            assert_eq!(spec.provider, ProviderId::Stability, "{id}");
            assert!(matches!(spec.auth, AuthScheme::Bearer), "{id}");
            assert!(!spec.beta, "{id}");
            assert!(!spec.capabilities.supports_boxes, "{id}");
            assert!(
                spec.endpoint.starts_with("https://api.stability.ai/"),
                "{id}"
            );
            assert!(crate::providers::ModelRegistry::providers().contains(&ProviderId::Stability));
            let resolved = provider(id);
            assert_eq!(resolved.spec().id, id, "{id}");
        }
        assert_eq!(
            spec_of("stability-erase").capabilities.image_slots,
            ImageSlots::List {
                max: 2,
                key: "image".into()
            }
        );
        assert_eq!(
            spec_of("stability-replace-background-and-relight")
                .capabilities
                .image_slots,
            ImageSlots::List {
                max: 3,
                key: "subject_image".into()
            }
        );
        assert_eq!(
            spec_of("stability-core").capabilities.image_slots,
            ImageSlots::None
        );
        for id in ["stability-ultra", "sd3.5-large", "stability-upscale-fast"] {
            assert_eq!(
                spec_of(id).capabilities.image_slots,
                ImageSlots::Single,
                "{id}"
            );
        }
    }

    #[test]
    fn sd3_switches_between_text_to_image_and_image_to_image() {
        let mut params = defaults("sd3.5-large");
        params.insert("aspect_ratio".into(), json!("16:9"));
        let text_to_image = submit("sd3.5-large", params.clone(), Vec::new());
        assert_eq!(
            text_to_image.url,
            "https://api.stability.ai/v2beta/stable-image/generate/sd3"
        );
        assert_eq!(
            part_names(&text_to_image),
            [
                "prompt",
                "mode",
                "model",
                "aspect_ratio",
                "cfg_scale",
                "output_format"
            ]
        );
        assert_eq!(text_value(&text_to_image, "mode"), "text-to-image");
        assert_eq!(text_value(&text_to_image, "model"), "sd3.5-large");
        assert_eq!(text_value(&text_to_image, "aspect_ratio"), "16:9");
        assert_eq!(text_value(&text_to_image, "cfg_scale"), "4");
        assert_eq!(text_value(&text_to_image, "output_format"), "png");
        assert_eq!(text_value(&text_to_image, "prompt"), "a lighthouse");
        assert!(
            parts_of(&text_to_image)
                .iter()
                .all(|part| part.name != "image")
        );
        assert!(
            parts_of(&text_to_image)
                .iter()
                .all(|part| part.name != "strength")
        );

        let image_to_image = submit("sd3.5-large", params, vec![encoded("image/png", 1)]);
        assert_eq!(
            part_names(&image_to_image),
            [
                "prompt",
                "mode",
                "model",
                "image",
                "strength",
                "cfg_scale",
                "output_format"
            ]
        );
        assert_eq!(text_value(&image_to_image, "mode"), "image-to-image");
        assert_eq!(text_value(&image_to_image, "strength"), "0.7");
        assert!(
            parts_of(&image_to_image)
                .iter()
                .all(|part| part.name != "aspect_ratio")
        );
        let part = binary_part(&image_to_image, "image");
        assert_eq!(part.filename.as_deref(), Some("image.png"));
        assert_eq!(part.content_type, "image/png");
        assert_eq!(part.bytes, vec![1; 16]);

        let ultra_t2i = submit("stability-ultra", defaults("stability-ultra"), Vec::new());
        assert_eq!(
            part_names(&ultra_t2i),
            ["prompt", "aspect_ratio", "output_format"]
        );
        let ultra_i2i = submit(
            "stability-ultra",
            defaults("stability-ultra"),
            vec![encoded("image/jpeg", 2)],
        );
        assert_eq!(
            part_names(&ultra_i2i),
            ["prompt", "image", "strength", "output_format"]
        );
        assert_eq!(text_value(&ultra_i2i, "strength"), "0.7");
        let part = binary_part(&ultra_i2i, "image");
        assert_eq!(part.filename.as_deref(), Some("image.jpg"));
        assert_eq!(part.content_type, "image/jpeg");
        assert!(parts_of(&ultra_i2i).iter().all(|part| part.name != "mode"));
    }

    #[test]
    fn sd3_defaults_carry_per_checkpoint_cfg_scale_and_strength() {
        for (id, cfg, strength) in [
            ("sd3.5-large", "4", "0.7"),
            ("sd3.5-large-turbo", "1", "0.7"),
            ("sd3.5-medium", "4", "0.7"),
            ("sd3.5-flash", "1", "0.95"),
        ] {
            let http = submit(id, defaults(id), vec![encoded("image/png", 1)]);
            assert_eq!(text_value(&http, "model"), id, "{id}");
            assert_eq!(text_value(&http, "cfg_scale"), cfg, "{id}");
            assert_eq!(text_value(&http, "strength"), strength, "{id}");
        }
    }

    #[test]
    fn strength_falls_back_to_the_schema_default_without_a_param() {
        let mut params = defaults("sd3.5-flash");
        params.remove("strength");
        let http = submit("sd3.5-flash", params, vec![encoded("image/png", 1)]);
        assert_eq!(text_value(&http, "strength"), "0.95");
    }

    #[test]
    fn fast_upscale_never_sends_a_prompt() {
        let scene = scene_with_image(512, 512);
        let errors = provider("stability-upscale-fast").validate_for_submit(
            &scene,
            &defaults("stability-upscale-fast"),
            1,
        );
        assert!(errors.is_empty(), "{errors:?}");

        let http = submit(
            "stability-upscale-fast",
            defaults("stability-upscale-fast"),
            vec![encoded("image/png", 1)],
        );
        assert_eq!(part_names(&http), ["image", "output_format"]);
        assert_eq!(text_value(&http, "output_format"), "png");
    }

    #[test]
    fn erase_and_inpaint_map_slots_to_image_and_mask() {
        let one = submit(
            "stability-erase",
            defaults("stability-erase"),
            vec![encoded("image/png", 1)],
        );
        assert_eq!(part_names(&one), ["image", "grow_mask", "output_format"]);
        assert_eq!(text_value(&one, "grow_mask"), "5");
        let two = submit(
            "stability-erase",
            defaults("stability-erase"),
            vec![encoded("image/png", 1), encoded("image/png", 2)],
        );
        assert_eq!(
            part_names(&two),
            ["image", "mask", "grow_mask", "output_format"]
        );
        assert_eq!(binary_part(&two, "mask").bytes, vec![2; 16]);
        let three = submit(
            "stability-erase",
            defaults("stability-erase"),
            vec![
                encoded("image/png", 1),
                encoded("image/png", 2),
                encoded("image/png", 3),
            ],
        );
        assert_eq!(
            part_names(&three),
            ["image", "mask", "grow_mask", "output_format"]
        );

        let inpaint = submit(
            "stability-inpaint",
            defaults("stability-inpaint"),
            vec![encoded("image/png", 1)],
        );
        assert_eq!(
            part_names(&inpaint),
            ["prompt", "image", "grow_mask", "output_format"]
        );
        let empty = scene_with_prose("  ");
        let errors = provider("stability-inpaint").validate_for_submit(
            &empty,
            &defaults("stability-inpaint"),
            1,
        );
        assert!(
            errors
                .iter()
                .any(|error| error.message == "Prompt is empty"),
            "{errors:?}"
        );
    }

    #[test]
    fn replace_background_maps_reference_slots_and_lighting_rules() {
        let provider = provider("stability-replace-background-and-relight");

        let mut values = defaults("stability-replace-background-and-relight");
        let one_image = scene_with_image(512, 512);
        let blocked = provider.validate_for_submit(&one_image, &values, 1);
        assert!(
            blocked
                .iter()
                .any(|error| error.key == "prompt" && error.message == "Prompt is empty"),
            "{blocked:?}"
        );
        values.insert("keep_original_background".into(), json!(true));
        let http = submit(
            "stability-replace-background-and-relight",
            values.clone(),
            vec![encoded("image/png", 1)],
        );
        assert_eq!(
            part_names(&http),
            [
                "background_prompt",
                "subject_image",
                "preserve_original_subject",
                "original_background_depth",
                "keep_original_background",
                "output_format"
            ]
        );
        assert_eq!(text_value(&http, "background_prompt"), "a lighthouse");
        assert_eq!(text_value(&http, "keep_original_background"), "true");
        assert_eq!(text_value(&http, "preserve_original_subject"), "0.6");
        assert_eq!(text_value(&http, "original_background_depth"), "0.5");
        assert!(
            parts_of(&http)
                .iter()
                .all(|part| part.name != "light_source_strength")
        );
        let mut described = scene_with_prose("a lighthouse");
        described.source_image = Some(ImageRef::new("canvas.png", 512, 512));
        let enabled = provider.validate_for_submit(&described, &values, 1);
        assert!(enabled.is_empty(), "{enabled:?}");

        let empty_prose = scene_with_prose("");
        let two_images = vec![encoded("image/png", 1), encoded("image/png", 2)];
        let quiet = defaults("stability-replace-background-and-relight");
        assert!(
            provider
                .validate_for_submit(&empty_prose, &quiet, 2)
                .is_empty(),
            "two images stand in for the background prompt"
        );
        let http = submit_with(
            "stability-replace-background-and-relight",
            "",
            quiet,
            two_images,
        );
        assert_eq!(
            part_names(&http),
            [
                "subject_image",
                "background_reference",
                "preserve_original_subject",
                "original_background_depth",
                "keep_original_background",
                "output_format"
            ]
        );
        assert_eq!(text_value(&http, "keep_original_background"), "false");
        assert!(
            parts_of(&http)
                .iter()
                .all(|part| part.name != "background_prompt")
        );
        assert!(
            parts_of(&http)
                .iter()
                .all(|part| part.name != "light_source_strength")
        );

        let three_images = vec![
            encoded("image/png", 1),
            encoded("image/png", 2),
            encoded("image/png", 3),
        ];
        let http = submit(
            "stability-replace-background-and-relight",
            defaults("stability-replace-background-and-relight"),
            three_images,
        );
        assert_eq!(
            part_names(&http),
            [
                "background_prompt",
                "subject_image",
                "background_reference",
                "light_reference",
                "preserve_original_subject",
                "original_background_depth",
                "keep_original_background",
                "light_source_strength",
                "output_format"
            ]
        );
        assert_eq!(text_value(&http, "light_source_strength"), "0.3");
        assert_eq!(binary_part(&http, "light_reference").bytes, vec![3; 16]);

        let mut lit = defaults("stability-replace-background-and-relight");
        lit.insert("light_source_direction".into(), json!("left"));
        let http = submit(
            "stability-replace-background-and-relight",
            lit,
            vec![encoded("image/png", 1)],
        );
        assert_eq!(
            part_names(&http),
            [
                "background_prompt",
                "subject_image",
                "preserve_original_subject",
                "original_background_depth",
                "keep_original_background",
                "light_source_direction",
                "light_source_strength",
                "output_format"
            ]
        );
        assert_eq!(text_value(&http, "light_source_strength"), "0.3");
    }

    #[test]
    fn outpaint_requires_a_direction_and_sends_down_not_bottom() {
        let provider = provider("stability-outpaint");
        let scene = scene_with_image(512, 512);
        let mut values = defaults("stability-outpaint");
        let blocked = provider.validate_for_submit(&scene, &values, 1);
        assert!(
            blocked
                .iter()
                .any(|error| error.message == "Set at least one outpaint direction above 0."),
            "{blocked:?}"
        );

        values.insert("down".into(), json!(200));
        assert!(
            provider.validate_for_submit(&scene, &values, 1).is_empty(),
            "{values:?}"
        );
        let http = submit_with(
            "stability-outpaint",
            "",
            values,
            vec![encoded("image/png", 1)],
        );
        assert_eq!(
            part_names(&http),
            [
                "image",
                "left",
                "right",
                "up",
                "down",
                "creativity",
                "output_format"
            ]
        );
        assert_eq!(text_value(&http, "down"), "200");
        assert_eq!(text_value(&http, "left"), "0");
        assert!(parts_of(&http).iter().all(|part| part.name != "bottom"));
        assert!(parts_of(&http).iter().all(|part| part.name != "prompt"));
        let voiced = submit_with(
            "stability-outpaint",
            "a lighthouse",
            defaults("stability-outpaint"),
            vec![encoded("image/png", 1)],
        );
        assert_eq!(part_names(&voiced)[0], "prompt");
    }

    #[test]
    fn search_prompts_are_required() {
        for (id, key, label) in [
            (
                "stability-search-and-replace",
                "search_prompt",
                "Search prompt is required",
            ),
            (
                "stability-search-and-recolor",
                "select_prompt",
                "Select prompt is required",
            ),
        ] {
            let provider = provider(id);
            let scene = scene_with_prose("make it pop");
            let mut values = defaults(id);
            let missing = provider.validate_for_submit(&scene, &values, 1);
            assert!(
                missing.iter().any(|error| error.message == label),
                "{id}: {missing:?}"
            );

            values.insert(key.into(), json!("   "));
            let blank = provider.validate_for_submit(&scene, &values, 1);
            assert!(
                blank.iter().any(|error| error.message == label),
                "{id}: {blank:?}"
            );

            values.insert(key.into(), json!("the red car"));
            assert!(
                provider.validate_for_submit(&scene, &values, 1).is_empty(),
                "{id}"
            );

            let http = submit(id, values, vec![encoded("image/png", 1)]);
            assert_eq!(
                part_names(&http),
                ["prompt", "image", key, "grow_mask", "output_format"]
            );
            assert_eq!(text_value(&http, key), "the red car");
            assert_eq!(text_value(&http, "grow_mask"), "3");
        }
    }

    #[test]
    fn remove_background_accepts_only_png_and_webp_output() {
        let spec = spec_of("stability-remove-background");
        assert!(spec.schema.accepts("output_format", &json!("png")));
        assert!(spec.schema.accepts("output_format", &json!("webp")));
        assert!(!spec.schema.accepts("output_format", &json!("jpeg")));
        let http = submit(
            "stability-remove-background",
            defaults("stability-remove-background"),
            vec![encoded("image/png", 1)],
        );
        assert_eq!(part_names(&http), ["image", "output_format"]);
    }

    #[test]
    fn every_model_posts_only_its_documented_parts() {
        let cases: Vec<(&str, BTreeMap<String, Value>, usize, &[&str])> = vec![
            (
                "stability-ultra",
                defaults("stability-ultra"),
                0,
                &["prompt", "aspect_ratio", "output_format"],
            ),
            (
                "stability-core",
                defaults("stability-core"),
                0,
                &["prompt", "aspect_ratio", "output_format"],
            ),
            (
                "sd3.5-large",
                defaults("sd3.5-large"),
                0,
                &[
                    "prompt",
                    "mode",
                    "model",
                    "aspect_ratio",
                    "cfg_scale",
                    "output_format",
                ],
            ),
            (
                "sd3.5-large-turbo",
                defaults("sd3.5-large-turbo"),
                0,
                &[
                    "prompt",
                    "mode",
                    "model",
                    "aspect_ratio",
                    "cfg_scale",
                    "output_format",
                ],
            ),
            (
                "sd3.5-medium",
                defaults("sd3.5-medium"),
                0,
                &[
                    "prompt",
                    "mode",
                    "model",
                    "aspect_ratio",
                    "cfg_scale",
                    "output_format",
                ],
            ),
            (
                "sd3.5-flash",
                defaults("sd3.5-flash"),
                0,
                &[
                    "prompt",
                    "mode",
                    "model",
                    "aspect_ratio",
                    "cfg_scale",
                    "output_format",
                ],
            ),
            (
                "stability-upscale-fast",
                defaults("stability-upscale-fast"),
                1,
                &["image", "output_format"],
            ),
            (
                "stability-upscale-conservative",
                defaults("stability-upscale-conservative"),
                1,
                &["prompt", "image", "creativity", "output_format"],
            ),
            (
                "stability-upscale-creative",
                defaults("stability-upscale-creative"),
                1,
                &["prompt", "image", "creativity", "output_format"],
            ),
            (
                "stability-outpaint",
                defaults("stability-outpaint"),
                1,
                &[
                    "prompt",
                    "image",
                    "left",
                    "right",
                    "up",
                    "down",
                    "creativity",
                    "output_format",
                ],
            ),
            (
                "stability-erase",
                defaults("stability-erase"),
                1,
                &["image", "grow_mask", "output_format"],
            ),
            (
                "stability-inpaint",
                defaults("stability-inpaint"),
                1,
                &["prompt", "image", "grow_mask", "output_format"],
            ),
            (
                "stability-search-and-replace",
                {
                    let mut values = defaults("stability-search-and-replace");
                    values.insert("search_prompt".into(), json!("the old sign"));
                    values
                },
                1,
                &[
                    "prompt",
                    "image",
                    "search_prompt",
                    "grow_mask",
                    "output_format",
                ],
            ),
            (
                "stability-search-and-recolor",
                {
                    let mut values = defaults("stability-search-and-recolor");
                    values.insert("select_prompt".into(), json!("the sky"));
                    values
                },
                1,
                &[
                    "prompt",
                    "image",
                    "select_prompt",
                    "grow_mask",
                    "output_format",
                ],
            ),
            (
                "stability-remove-background",
                defaults("stability-remove-background"),
                1,
                &["image", "output_format"],
            ),
            (
                "stability-replace-background-and-relight",
                defaults("stability-replace-background-and-relight"),
                1,
                &[
                    "background_prompt",
                    "subject_image",
                    "preserve_original_subject",
                    "original_background_depth",
                    "keep_original_background",
                    "output_format",
                ],
            ),
        ];
        for (id, params, images, expected) in cases {
            let images: Vec<_> = (0..images)
                .map(|seed| encoded("image/png", seed as u8 + 1))
                .collect();
            let http = submit(id, params, images);
            assert_eq!(http.method, Method::Post, "{id}");
            assert_eq!(http.auth, Some(ProviderId::Stability), "{id}");
            assert_eq!(
                http.headers,
                vec![HttpHeader {
                    name: "Accept".into(),
                    value: "image/*".into(),
                    sensitive: false,
                }],
                "{id}"
            );
            assert_eq!(part_names(&http), expected, "{id}");
        }
    }

    #[test]
    fn optional_fields_are_omitted_when_unset_or_empty() {
        let mut params = defaults("stability-core");
        params.insert("seed".into(), json!(0));
        params.insert("negative_prompt".into(), json!(""));
        let http = submit("stability-core", params, Vec::new());
        assert_eq!(
            part_names(&http),
            ["prompt", "aspect_ratio", "output_format"]
        );

        let mut params = defaults("stability-core");
        params.insert("seed".into(), json!(42));
        params.insert("negative_prompt".into(), json!("fog"));
        params.insert("style_preset".into(), json!("anime"));
        params.insert("aspect_ratio".into(), json!("9:16"));
        let http = submit("stability-core", params, Vec::new());
        assert_eq!(
            part_names(&http),
            [
                "prompt",
                "aspect_ratio",
                "negative_prompt",
                "seed",
                "style_preset",
                "output_format"
            ]
        );
        assert_eq!(text_value(&http, "seed"), "42");
        assert_eq!(text_value(&http, "negative_prompt"), "fog");
        assert_eq!(text_value(&http, "style_preset"), "anime");
        assert_eq!(text_value(&http, "aspect_ratio"), "9:16");
    }

    #[test]
    fn build_submit_ignores_the_resolved_image_inputs() {
        let http = provider("stability-ultra").build_submit(
            &request(
                "stability-ultra",
                defaults("stability-ultra"),
                vec![encoded("image/png", 7)],
            ),
            &[ImageInput::RawBase64("should-be-ignored".into())],
        );
        let part = binary_part(&http, "image");
        assert_eq!(part.bytes, vec![7; 16]);
        assert_eq!(part.filename.as_deref(), Some("image.png"));
    }

    #[test]
    fn input_image_bounds_follow_the_model_table() {
        let validate = |id: &str, width: u32, height: u32| {
            let mut scene = scene_with_prose("make it sharper");
            scene.source_image = Some(ImageRef::new("canvas.png", width, height));
            provider(id).validate_for_submit(&scene, &defaults(id), 1)
        };
        for id in [
            "stability-upscale-fast",
            "stability-upscale-conservative",
            "stability-upscale-creative",
            "stability-outpaint",
            "stability-erase",
            "stability-inpaint",
            "stability-search-and-replace",
            "stability-search-and-recolor",
            "stability-remove-background",
            "stability-replace-background-and-relight",
        ] {
            let mut values = defaults(id);
            if id == "stability-outpaint" {
                values.insert("down".into(), json!(200));
            }
            let errors = provider(id).validate_for_submit(&scene_with_image(0, 0), &values, 1);
            assert!(
                !errors.iter().any(|error| error.key == "images"),
                "{id}: unknown sizes are skipped, {errors:?}"
            );
        }

        assert!(!validate("stability-upscale-fast", 20, 20).is_empty());
        assert!(validate("stability-upscale-fast", 48, 48).is_empty());
        assert!(!validate("stability-upscale-conservative", 48, 48).is_empty());

        assert!(!validate("stability-upscale-fast", 1600, 1000).is_empty());
        assert!(!validate("stability-upscale-creative", 1600, 1000).is_empty());
        assert!(validate("stability-upscale-conservative", 1600, 1000).is_empty());

        let mut outpaint = defaults("stability-outpaint");
        outpaint.insert("down".into(), json!(200));
        let ratio = provider("stability-outpaint").validate_for_submit(
            &scene_with_image(300, 1000),
            &outpaint,
            1,
        );
        assert!(!ratio.is_empty(), "300x1000 breaks the 1:2.5 ratio floor");
        let conservative = validate("stability-upscale-conservative", 300, 1000);
        assert!(!conservative.is_empty(), "{conservative:?}");

        assert!(!validate("stability-ultra", 20, 20).is_empty());
        assert!(validate("stability-ultra", 512, 512).is_empty());
        assert!(!validate("sd3.5-large", 20, 20).is_empty());
        assert!(validate("sd3.5-large", 512, 512).is_empty());
    }

    #[test]
    fn the_replace_background_ratio_rule_covers_only_the_subject() {
        let mut values = defaults("stability-replace-background-and-relight");
        let mut scene = scene_with_image(300, 1000);
        scene.references = vec![
            ImageRef::new("background.png", 512, 512),
            ImageRef::new("light.png", 512, 512),
        ];
        let errors = provider("stability-replace-background-and-relight")
            .validate_for_submit(&scene, &values, 3);
        assert!(!errors.is_empty(), "the 1:3.33 subject is refused");
        values.insert("background_prompt".into(), json!("bright sky"));
        scene.source_image = Some(ImageRef::new("subject.png", 512, 512));
        scene.references = vec![
            ImageRef::new("background.png", 300, 1000),
            ImageRef::new("light.png", 512, 512),
        ];
        let errors = provider("stability-replace-background-and-relight")
            .validate_for_submit(&scene, &values, 3);
        assert!(
            !errors.iter().any(|error| error.key == "images"),
            "references escape the ratio rule: {errors:?}"
        );
    }

    #[test]
    fn a_matching_image_200_becomes_the_inline_result() {
        for (content_type, bytes, mime) in [
            ("image/png", PNG_MAGIC, "image/png"),
            ("image/jpeg", JPEG_MAGIC, "image/jpeg"),
            ("image/webp", WEBP_BYTES, "image/webp"),
            ("image/jpg; charset=binary", JPEG_MAGIC, "image/jpeg"),
        ] {
            let provider = provider("stability-ultra");
            let inline = provider
                .inline_image(&response(200, Some(content_type), bytes))
                .unwrap_or_else(|| panic!("{content_type}"));
            assert_eq!(inline.bytes, bytes);
            assert_eq!(inline.mime, mime);
            assert_eq!(inline.interaction_id, "");
            assert_eq!(inline.suggestion, "");
            assert_eq!(
                provider
                    .parse_submit(&response(200, Some(content_type), bytes))
                    .unwrap_err(),
                ProviderError::Protocol(
                    "stability returns the image on the submit response".into()
                )
            );
        }
    }

    #[test]
    fn json_image_payloads_decode_to_inline_images() {
        let encoded = base64::engine::general_purpose::STANDARD.encode(PNG_MAGIC);
        let body = format!(r#"{{"image":"{encoded}","finish_reason":"SUCCESS","seed":7}}"#);
        let inline = provider("stability-ultra")
            .inline_image(&response(
                200,
                Some("application/json; type=image/png"),
                body.as_bytes(),
            ))
            .unwrap();
        assert_eq!(inline.bytes, PNG_MAGIC);
        assert_eq!(inline.mime, "image/png");
    }

    #[test]
    fn content_filtered_and_moderated_responses_block() {
        let provider = provider("stability-ultra");
        let header = header_response(
            200,
            "image/png",
            PNG_MAGIC,
            "finish-reason",
            "CONTENT_FILTERED",
        );
        let lowercase = header_response(
            200,
            "image/png",
            PNG_MAGIC,
            "Finish-Reason",
            "content_filtered",
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(PNG_MAGIC);
        let json_body = format!(r#"{{"image":"{encoded}","finish_reason":"CONTENT_FILTERED"}}"#);
        let json = response(200, Some("application/json"), json_body.as_bytes());
        let moderated = response(
            200,
            Some("application/json"),
            br#"{"id":"abc","name":"content_moderation","errors":["flagged"]}"#,
        );
        let forbidden = response(
            403,
            Some("application/json"),
            br#"{"name":"content_moderation"}"#,
        );
        for (kind, candidate) in [
            ("header", &header),
            ("lowercase-header", &lowercase),
            ("json", &json),
            ("name", &moderated),
            ("403", &forbidden),
        ] {
            assert!(provider.inline_image(candidate).is_none(), "{kind}");
            assert_eq!(
                provider.parse_submit(candidate).unwrap_err(),
                ProviderError::Blocked,
                "{kind}"
            );
            assert_eq!(
                provider.parse_poll(candidate),
                PollOutcome::Failed(ProviderError::Blocked),
                "{kind}"
            );
        }
        let success = header_response(200, "image/png", PNG_MAGIC, "finish-reason", "SUCCESS");
        assert!(provider.inline_image(&success).is_some());
    }

    #[test]
    fn http_errors_classify_per_the_failure_table() {
        let provider = provider("stability-ultra");
        assert_eq!(
            provider
                .parse_submit(&response(401, None, b"nope"))
                .unwrap_err(),
            ProviderError::Auth
        );
        assert_eq!(
            provider
                .parse_submit(&response(404, None, b"nope"))
                .unwrap_err(),
            ProviderError::ModelNotFound
        );
        assert_eq!(
            provider
                .parse_submit(&response(429, None, b"slow down"))
                .unwrap_err(),
            ProviderError::RateLimited { retry_after: None }
        );
        assert_eq!(
            provider
                .parse_submit(&response(402, None, b"no credits"))
                .unwrap_err(),
            ProviderError::OutOfCredits { hint: None }
        );
        for status in [400, 413, 422] {
            let body = br#"{"id":"1","name":"bad_request","errors":["some-field: is required"]}"#;
            assert_eq!(
                provider
                    .parse_submit(&response(status, None, body))
                    .unwrap_err(),
                ProviderError::Validation {
                    message: "some-field: is required".into(),
                    field_errors: Vec::new(),
                },
                "{status}"
            );
        }
        assert_eq!(
            provider
                .parse_submit(&response(400, None, b"not json"))
                .unwrap_err(),
            ProviderError::Http {
                status: 400,
                body: "not json".into(),
            }
        );
        assert_eq!(
            provider
                .parse_submit(&response(500, None, b"boom"))
                .unwrap_err(),
            ProviderError::Http {
                status: 500,
                body: "boom".into(),
            }
        );
    }

    #[test]
    fn a_200_without_image_bytes_is_no_image() {
        let provider = provider("stability-ultra");
        for (kind, content_type, body) in [
            ("html", Some("text/html"), b"<html>hi</html>".as_slice()),
            ("empty", Some("image/png"), b"".as_slice()),
            ("magic-mismatch", Some("image/png"), JPEG_MAGIC),
            ("not-an-image", Some("image/png"), b"nope".as_slice()),
            ("no-header", None, PNG_MAGIC),
            (
                "json-without-image",
                Some("application/json"),
                br#"{"id":"1"}"#.as_slice(),
            ),
        ] {
            let candidate = response(200, content_type, body);
            assert!(provider.inline_image(&candidate).is_none(), "{kind}");
            assert_eq!(
                provider.parse_submit(&candidate).unwrap_err(),
                ProviderError::NoImage,
                "{kind}"
            );
            assert_eq!(
                provider.parse_poll(&candidate),
                PollOutcome::Failed(ProviderError::NoImage),
                "{kind}"
            );
        }
    }

    #[test]
    fn async_submit_parses_the_generation_id() {
        let id = "a".repeat(64);
        let body = format!(r#"{{"id":"{id}","name":"generation_started"}}"#);
        for model in [
            "stability-upscale-creative",
            "stability-replace-background-and-relight",
        ] {
            let provider = provider(model);
            let submitted = provider
                .parse_submit(&response(200, Some("application/json"), body.as_bytes()))
                .unwrap_or_else(|error| panic!("{model}: {error:?}"));
            assert_eq!(submitted.job_id.as_str(), id);
            assert_eq!(
                submitted.poll_url.as_str(),
                format!("https://api.stability.ai/v2beta/results/{id}")
            );
            assert_eq!(submitted.quote, None);

            let poll = provider.poll_request(&submitted);
            assert_eq!(poll.method, Method::Get);
            assert_eq!(
                poll.url,
                format!("https://api.stability.ai/v2beta/results/{id}")
            );
            assert_eq!(poll.auth, Some(ProviderId::Stability));
            assert_eq!(poll.body, Body::Empty);
            assert_eq!(poll.max_body_bytes, 100 * 1024 * 1024);
            assert!(
                poll.headers
                    .iter()
                    .any(|header| header.name == "Accept" && header.value == "image/*")
            );

            let policy = provider.poll_policy();
            assert_eq!(policy.initial, Duration::from_secs(10));
            assert!((policy.factor - 1.0).abs() < 1e-9);
            assert_eq!(policy.cap, Duration::from_secs(10));
            assert_eq!(policy.deadline, Duration::from_secs(600));
        }
        let provider = provider("stability-upscale-creative");
        assert!(matches!(
            provider
                .parse_submit(&response(
                    200,
                    Some("application/json"),
                    br#"{"id":"bad id!"}"#
                ))
                .unwrap_err(),
            ProviderError::Protocol(_)
        ));
        assert!(matches!(
            provider
                .parse_submit(&response(200, Some("application/json"), br#"{}"#))
                .unwrap_err(),
            ProviderError::Protocol(_)
        ));
    }

    #[test]
    fn poll_202_stays_in_flight_and_errors_classify_for_polling() {
        let provider = provider("stability-upscale-creative");
        assert_eq!(
            provider.parse_poll(&response(
                202,
                Some("application/json"),
                br#"{"id":"a","status":"in-progress"}"#
            )),
            PollOutcome::InFlight {
                status: "in-progress".into(),
                progress: None,
            }
        );
        assert_eq!(
            provider.parse_poll(&response(404, None, b"gone")),
            PollOutcome::Failed(ProviderError::Protocol(
                "generation not found or expired".into()
            ))
        );
        assert_eq!(
            provider.parse_poll(&response(401, None, b"nope")),
            PollOutcome::Failed(ProviderError::Auth)
        );
        assert_eq!(
            provider.parse_poll(&response(429, None, b"slow down")),
            PollOutcome::Failed(ProviderError::RateLimited { retry_after: None })
        );
        assert_eq!(
            provider.parse_poll(&response(402, None, b"no credits")),
            PollOutcome::Failed(ProviderError::OutOfCredits { hint: None })
        );
        assert_eq!(
            provider.parse_poll(&response(
                400,
                None,
                br#"{"id":"1","name":"bad_request","errors":["image: is required"]}"#
            )),
            PollOutcome::Failed(ProviderError::Validation {
                message: "image: is required".into(),
                field_errors: Vec::new(),
            })
        );
        assert_eq!(
            provider.parse_poll(&response(500, None, b"boom")),
            PollOutcome::Failed(ProviderError::Http {
                status: 500,
                body: "boom".into(),
            })
        );
    }
}
