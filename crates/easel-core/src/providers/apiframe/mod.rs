use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::error::{FieldError, ProviderError};
use crate::http::{Body, HttpHeader, HttpRequest, HttpResponse, Method, Part};
use crate::job::{EncodedImage, JobRequest, PollPolicy};
use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::providers::{ImageInput, JobId, PollOutcome, ReferenceStep, Submitted};
use crate::scene::Scene;
use crate::schema::{FieldDef, FieldKind, ParamSchema, RuleContext};

pub const GENERATE_URL: &str = "https://api.apiframe.ai/v2/images/generate";
pub const UPLOAD_URL: &str = "https://api.apiframe.ai/v2/uploads";
const JOBS_URL: &str = "https://api.apiframe.ai/v2/jobs";
const MAX_PROMPT_CHARS: usize = 2000;
const STORED_CAP: usize = 64 * 1024;

/// A descriptor-backed Apiframe model. The descriptor carries the wire model
/// string, the family parameter wrapper key, and the reference-image slot rule.
#[derive(Debug, Clone, Copy)]
pub struct Apiframe {
    descriptor: &'static Descriptor,
}

#[derive(Debug)]
pub struct Descriptor {
    pub(crate) spec: ModelSpec,
    wire_id: &'static str,
    params_key: Option<&'static str>,
    slot: Slot,
}

#[derive(Debug, Clone, Copy)]
enum Slot {
    None,
    Single { field: &'static str },
    List { max: usize, field: &'static str },
}

impl Slot {
    fn image_slots(self) -> ImageSlots {
        match self {
            Self::None => ImageSlots::None,
            Self::Single { .. } => ImageSlots::Single,
            Self::List { max, field } => ImageSlots::List {
                max,
                key: (*field).into(),
            },
        }
    }
}

fn apiframe_spec(
    id: &str,
    display_name: &str,
    slots: ImageSlots,
    fields: Vec<FieldDef>,
) -> ModelSpec {
    ModelSpec {
        id: id.into(),
        display_name: display_name.into(),
        provider: ProviderId::Apiframe,
        auth: AuthScheme::HeaderKey {
            header: "X-API-Key".into(),
        },
        endpoint: GENERATE_URL.into(),
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

macro_rules! apiframe_model {
    ($constructor:ident, $descriptor:ident, $id:literal, $name:literal, $wire:literal, $params_key:expr, $slot:expr, $fields:expr) => {
        static $descriptor: LazyLock<Descriptor> = LazyLock::new(|| Descriptor {
            spec: apiframe_spec($id, $name, $slot.image_slots(), $fields),
            wire_id: $wire,
            params_key: $params_key,
            slot: $slot,
        });

        #[must_use]
        pub fn $constructor() -> Apiframe {
            Apiframe {
                descriptor: &$descriptor,
            }
        }
    };
}

const AR8: &[&str] = &["1:1", "16:9", "9:16", "4:3", "3:4", "3:2", "2:3", "21:9"];
const AR8_MATCH: &[&str] = &[
    "match_input_image",
    "1:1",
    "16:9",
    "9:16",
    "4:3",
    "3:4",
    "3:2",
    "2:3",
    "21:9",
];
const AR8_CUSTOM: &[&str] = &[
    "1:1", "16:9", "9:16", "4:3", "3:4", "3:2", "2:3", "21:9", "custom",
];
const AR8_MATCH_CUSTOM: &[&str] = &[
    "1:1",
    "16:9",
    "9:16",
    "4:3",
    "3:4",
    "3:2",
    "2:3",
    "21:9",
    "match_input_image",
    "custom",
];
const IDEOGRAM_V2_RATIOS: &[&str] = &[
    "1:1", "16:9", "9:16", "3:2", "2:3", "4:3", "5:4", "3:5", "10:16", "16:10", "1:3",
];
const IDEOGRAM_V3_RATIOS: &[&str] = &[
    "1:1", "16:9", "9:16", "4:3", "3:4", "3:2", "2:3", "16:10", "10:16", "3:1", "1:3",
];
const IMAGEN_RATIOS: &[&str] = &["1:1", "9:16", "16:9", "3:4", "4:3"];
const SD_RATIOS: &[&str] = &[
    "1:1", "16:9", "21:9", "3:2", "2:3", "4:5", "5:4", "9:16", "9:21",
];
const WAN_RATIOS: &[&str] = &["1:1", "2:3", "3:2", "3:4", "4:3", "9:16", "16:9", "21:9"];

fn select_field(key: &str, label: &str, options: &[&str], default: &str) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: None,
        kind: FieldKind::Select {
            options: options.iter().map(|option| (*option).to_owned()).collect(),
        },
        required: false,
        default: Some(json!(default)),
        locked_when: None,
    }
}

fn int_field(key: &str, label: &str, min: i64, max: i64, default: Option<i64>) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: None,
        kind: FieldKind::Int { min, max, step: 1 },
        required: false,
        default: default.map(|value| json!(value)),
        locked_when: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn float_field(key: &str, label: &str, min: f64, max: f64, step: f64, default: f64) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: None,
        kind: FieldKind::Float { min, max, step },
        required: false,
        default: Some(json!(default)),
        locked_when: None,
    }
}

fn bool_field(key: &str, label: &str, default: bool) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: None,
        kind: FieldKind::Bool,
        required: false,
        default: Some(json!(default)),
        locked_when: None,
    }
}

fn text_field(key: &str, label: &str) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: None,
        kind: FieldKind::Text { multiline: false },
        required: false,
        default: None,
        locked_when: None,
    }
}

fn seed_field() -> FieldDef {
    int_field("seed", "Seed", 0, 2_147_483_647, None)
}

fn aspect_field(options: &[&str], default: &str) -> FieldDef {
    select_field("aspect_ratio", "Aspect ratio", options, default)
}

fn output_format_field(options: &[&str], default: &str) -> FieldDef {
    select_field("output_format", "Output format", options, default)
}

fn negative_prompt_field() -> FieldDef {
    text_field("negative_prompt", "Negative prompt")
}

fn output_quality_field() -> FieldDef {
    int_field("output_quality", "Output quality", 0, 100, Some(90))
}

fn magic_prompt_field() -> FieldDef {
    select_field(
        "magic_prompt_option",
        "Magic prompt",
        &["Auto", "On", "Off"],
        "Auto",
    )
}

fn midjourney_fields() -> Vec<FieldDef> {
    vec![aspect_field(AR8, "1:1")]
}

fn nano_banana_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8, "1:1"),
        output_format_field(&["png", "jpg", "webp"], "png"),
    ]
}

fn nano_banana_pro_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_MATCH, "1:1"),
        select_field(
            "resolution",
            "Resolution",
            &["512px", "1K", "2K", "4K"],
            "2K",
        ),
        output_format_field(&["jpg", "png", "webp"], "jpg"),
    ]
}

fn nano_banana_2_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_MATCH, "1:1"),
        select_field(
            "resolution",
            "Resolution",
            &["512px", "1K", "2K", "4K"],
            "1K",
        ),
        output_format_field(&["jpg", "png"], "jpg"),
        bool_field("google_search", "Google Search", false),
        bool_field("image_search", "Image Search", false),
    ]
}

fn nano_banana_2_lite_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_MATCH, "1:1"),
        output_format_field(&["jpg", "png"], "jpg"),
        bool_field("google_search", "Google Search", false),
        bool_field("image_search", "Image Search", false),
    ]
}

fn flux_11_pro_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_CUSTOM, "1:1"),
        int_field("width", "Width", 256, 1440, None),
        int_field("height", "Height", 256, 1440, None),
        output_format_field(&["jpg", "png", "webp"], "jpg"),
        output_quality_field(),
        bool_field("prompt_upsampling", "Prompt upsampling", false),
        seed_field(),
    ]
}

fn flux_11_ultra_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8, "1:1"),
        bool_field("raw", "Raw mode", false),
        float_field(
            "image_prompt_strength",
            "Image prompt strength",
            0.0,
            1.0,
            0.1,
            0.1,
        ),
        output_format_field(&["jpg", "png", "webp"], "jpg"),
        seed_field(),
    ]
}

fn flux_resolution_field() -> FieldDef {
    select_field(
        "resolution",
        "Resolution",
        &["0.5 MP", "1 MP", "2 MP", "4 MP"],
        "1 MP",
    )
}

fn flux_2_pro_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_MATCH_CUSTOM, "1:1"),
        flux_resolution_field(),
        int_field("width", "Width", 256, 2048, None),
        int_field("height", "Height", 256, 2048, None),
        output_format_field(&["jpg", "png", "webp"], "jpg"),
        output_quality_field(),
        bool_field("prompt_upsampling", "Prompt upsampling", false),
        seed_field(),
    ]
}

fn flux_2_dev_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_MATCH_CUSTOM, "1:1"),
        int_field("width", "Width", 256, 1440, None),
        int_field("height", "Height", 256, 1440, None),
        bool_field("go_fast", "Go fast", true),
        output_format_field(&["jpg", "png", "webp"], "jpg"),
        output_quality_field(),
        seed_field(),
    ]
}

fn flux_2_flex_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_MATCH_CUSTOM, "1:1"),
        flux_resolution_field(),
        int_field("width", "Width", 256, 2048, None),
        int_field("height", "Height", 256, 2048, None),
        int_field("steps", "Steps", 1, 50, Some(30)),
        float_field("guidance", "Guidance", 1.5, 10.0, 0.5, 4.5),
        bool_field("prompt_upsampling", "Prompt upsampling", false),
        output_format_field(&["jpg", "png", "webp"], "jpg"),
        output_quality_field(),
        seed_field(),
    ]
}

fn flux_2_max_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_MATCH_CUSTOM, "1:1"),
        flux_resolution_field(),
        int_field("width", "Width", 256, 2048, None),
        int_field("height", "Height", 256, 2048, None),
        output_format_field(&["jpg", "png", "webp"], "jpg"),
        output_quality_field(),
        bool_field("prompt_upsampling", "Prompt upsampling", false),
        seed_field(),
    ]
}

fn seedream_3_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_CUSTOM, "1:1"),
        select_field("size", "Size", &["small", "regular", "big"], "regular"),
        int_field("width", "Width", 512, 2048, None),
        int_field("height", "Height", 512, 2048, None),
        float_field("guidance_scale", "Guidance", 1.0, 10.0, 0.5, 2.5),
        seed_field(),
    ]
}

fn seedream_3_1_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_CUSTOM, "1:1"),
        select_field("resolution", "Resolution", &["1K", "2K"], "2K"),
        int_field("width", "Width", 512, 3024, None),
        int_field("height", "Height", 512, 3024, None),
        bool_field("use_pre_llm", "Use pre LLM", true),
        seed_field(),
    ]
}

fn seedream_4_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_MATCH, "1:1"),
        select_field("size", "Size", &["1K", "2K", "4K", "custom"], "2K"),
        int_field("width", "Width", 1024, 4096, None),
        int_field("height", "Height", 1024, 4096, None),
        bool_field("enhance_prompt", "Enhance prompt", true),
    ]
}

fn seedream_4_5_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_MATCH, "1:1"),
        select_field("size", "Size", &["2K", "4K", "custom"], "2K"),
        int_field("width", "Width", 1024, 4096, None),
        int_field("height", "Height", 1024, 4096, None),
    ]
}

fn seedream_5_lite_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_MATCH, "1:1"),
        select_field("size", "Size", &["2K", "3K"], "2K"),
        output_format_field(&["png", "jpg"], "png"),
    ]
}

fn seedream_5_pro_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(AR8_MATCH, "1:1"),
        select_field("size", "Size", &["1K", "2K"], "2K"),
        output_format_field(&["png", "jpg"], "png"),
        seed_field(),
    ]
}

fn ideogram_v2_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(IDEOGRAM_V2_RATIOS, "1:1"),
        select_field(
            "style_type",
            "Style type",
            &[
                "Auto",
                "General",
                "Realistic",
                "Design",
                "Render_3d",
                "Anime",
                "None",
            ],
            "Auto",
        ),
        magic_prompt_field(),
        negative_prompt_field(),
        seed_field(),
    ]
}

fn ideogram_v3_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(IDEOGRAM_V3_RATIOS, "1:1"),
        select_field(
            "style_type",
            "Style type",
            &["Auto", "General", "Realistic", "Design", "None"],
            "Auto",
        ),
        magic_prompt_field(),
        text_field("style_preset", "Style preset"),
        seed_field(),
    ]
}

fn ideogram_character_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(IDEOGRAM_V3_RATIOS, "1:1"),
        select_field(
            "style_type",
            "Style type",
            &["Auto", "Fiction", "Realistic"],
            "Auto",
        ),
        select_field(
            "rendering_speed",
            "Rendering speed",
            &["turbo", "default", "quality"],
            "default",
        ),
        magic_prompt_field(),
        seed_field(),
    ]
}

fn sd_base_fields(
    cfg_default: f64,
    steps_max: i64,
    steps_default: i64,
    with_negative: bool,
) -> Vec<FieldDef> {
    let mut fields = vec![
        aspect_field(SD_RATIOS, "1:1"),
        float_field("cfg", "CFG", 0.0, 20.0, 0.5, cfg_default),
        int_field("steps", "Steps", 1, steps_max, Some(steps_default)),
        float_field("prompt_strength", "Prompt strength", 0.0, 1.0, 0.05, 0.85),
    ];
    if with_negative {
        fields.push(negative_prompt_field());
    }
    fields.extend([
        output_format_field(&["webp", "jpg", "png"], "webp"),
        output_quality_field(),
        seed_field(),
    ]);
    fields
}

fn sd35_large_fields() -> Vec<FieldDef> {
    sd_base_fields(3.5, 40, 28, true)
}

fn sd35_turbo_fields() -> Vec<FieldDef> {
    sd_base_fields(1.0, 40, 4, false)
}

fn sd3_fields() -> Vec<FieldDef> {
    sd_base_fields(3.5, 28, 28, true)
}

fn imagen_3_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(IMAGEN_RATIOS, "1:1"),
        output_format_field(&["jpg", "png"], "jpg"),
    ]
}

fn imagen_4_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(IMAGEN_RATIOS, "1:1"),
        select_field("image_size", "Image size", &["1K", "2K"], "1K"),
        output_format_field(&["jpg", "png"], "jpg"),
    ]
}

fn dalle_3_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(&["1:1", "3:2", "2:3"], "1:1"),
        select_field("style", "Style", &["vivid", "natural"], "vivid"),
    ]
}

fn gpt_image_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(&["1:1", "3:2", "2:3"], "1:1"),
        select_field(
            "quality",
            "Quality",
            &["auto", "low", "medium", "high"],
            "auto",
        ),
        select_field(
            "background",
            "Background",
            &["auto", "transparent", "opaque"],
            "auto",
        ),
        select_field("input_fidelity", "Input fidelity", &["low", "high"], "low"),
        output_format_field(&["webp", "png", "jpeg"], "webp"),
    ]
}

fn kling_fields() -> Vec<FieldDef> {
    vec![
        select_field(
            "model_name",
            "Model",
            &["kling-v1", "kling-v1-5", "kling-v2"],
            "kling-v1",
        ),
        aspect_field(&["16:9", "9:16", "1:1", "4:3", "3:4", "3:2", "2:3"], "1:1"),
        float_field("image_fidelity", "Image fidelity", 0.0, 1.0, 0.1, 0.5),
        negative_prompt_field(),
    ]
}

fn qwen_image_fields() -> Vec<FieldDef> {
    vec![
        select_field(
            "task_type",
            "Task type",
            &["text-to-image", "image-to-image", "image-edit"],
            "text-to-image",
        ),
        select_field(
            "image_size",
            "Image size",
            &[
                "square",
                "square_hd",
                "portrait_4_3",
                "portrait_16_9",
                "landscape_4_3",
                "landscape_16_9",
            ],
            "square_hd",
        ),
        int_field("num_inference_steps", "Steps", 2, 250, Some(30)),
        float_field("guidance_scale", "Guidance", 0.0, 20.0, 0.5, 4.0),
        float_field("strength", "Strength", 0.0, 1.0, 0.1, 0.8),
        select_field(
            "acceleration",
            "Acceleration",
            &["none", "regular", "high"],
            "none",
        ),
        negative_prompt_field(),
        output_format_field(&["png", "jpeg"], "png"),
        seed_field(),
    ]
}

fn qwen_image_2_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(
            &[
                "1:1", "2:3", "3:2", "3:4", "4:3", "9:16", "16:9", "21:9", "2:1", "1:2",
            ],
            "1:1",
        ),
        negative_prompt_field(),
        seed_field(),
    ]
}

fn wan_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(WAN_RATIOS, "1:1"),
        bool_field("prompt_extend", "Prompt extend", false),
        negative_prompt_field(),
        seed_field(),
    ]
}

fn wan_27_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(WAN_RATIOS, "1:1"),
        bool_field("thinking_mode", "Thinking mode", true),
        seed_field(),
    ]
}

fn grok_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(
            &[
                "auto", "1:1", "16:9", "9:16", "4:3", "3:4", "3:2", "2:3", "2:1", "1:2", "19.5:9",
                "9:19.5", "20:9", "9:20",
            ],
            "1:1",
        ),
        select_field("resolution", "Resolution", &["1k", "2k"], "2k"),
        select_field("quality", "Quality", &["low", "medium"], "medium"),
    ]
}

fn grok_fast_fields() -> Vec<FieldDef> {
    vec![aspect_field(&["1:1", "2:3", "3:2", "16:9", "9:16"], "1:1")]
}

fn reve_fields() -> Vec<FieldDef> {
    vec![
        aspect_field(&["3:2", "16:9", "9:16", "2:3", "4:3", "3:4", "1:1"], "3:2"),
        select_field(
            "version",
            "Version",
            &["latest", "reve-create@20250915"],
            "latest",
        ),
        seed_field(),
    ]
}

apiframe_model!(
    midjourney,
    MIDJOURNEY,
    "apiframe-midjourney",
    "Midjourney",
    "midjourney",
    Some("midjourneyParams"),
    Slot::None,
    midjourney_fields()
);
apiframe_model!(
    nano_banana,
    NANO_BANANA,
    "apiframe-nano-banana",
    "Nano Banana",
    "nano-banana",
    Some("nanoBananaParams"),
    Slot::List {
        max: 14,
        field: "image_input"
    },
    nano_banana_fields()
);
apiframe_model!(
    nano_banana_pro,
    NANO_BANANA_PRO,
    "apiframe-nano-banana-pro",
    "Nano Banana Pro",
    "nano-banana-pro",
    Some("nanoBananaParams"),
    Slot::List {
        max: 14,
        field: "image_input"
    },
    nano_banana_pro_fields()
);
apiframe_model!(
    nano_banana_2,
    NANO_BANANA_2,
    "apiframe-nano-banana-2",
    "Nano Banana 2",
    "nano-banana-2",
    Some("nanoBananaParams"),
    Slot::List {
        max: 14,
        field: "image_input"
    },
    nano_banana_2_fields()
);
apiframe_model!(
    nano_banana_2_lite,
    NANO_BANANA_2_LITE,
    "apiframe-nano-banana-2-lite",
    "Nano Banana 2 Lite",
    "nano-banana-2-lite",
    Some("nanoBananaParams"),
    Slot::List {
        max: 14,
        field: "image_input"
    },
    nano_banana_2_lite_fields()
);
apiframe_model!(
    flux_11_pro,
    FLUX_11_PRO,
    "apiframe-flux-1.1-pro",
    "Flux 1.1 Pro",
    "flux-1.1-pro",
    Some("fluxParams"),
    Slot::Single {
        field: "image_prompt"
    },
    flux_11_pro_fields()
);
apiframe_model!(
    flux_11_pro_ultra,
    FLUX_11_PRO_ULTRA,
    "apiframe-flux-1.1-pro-ultra",
    "Flux 1.1 Pro Ultra",
    "flux-1.1-pro-ultra",
    Some("fluxParams"),
    Slot::Single {
        field: "image_prompt"
    },
    flux_11_ultra_fields()
);
apiframe_model!(
    flux_2_pro,
    FLUX_2_PRO,
    "apiframe-flux-2-pro",
    "Flux 2 Pro",
    "flux-2-pro",
    Some("fluxParams"),
    Slot::List {
        max: 8,
        field: "input_images"
    },
    flux_2_pro_fields()
);
apiframe_model!(
    flux_2_dev,
    FLUX_2_DEV,
    "apiframe-flux-2-dev",
    "Flux 2 Dev",
    "flux-2-dev",
    Some("fluxParams"),
    Slot::List {
        max: 5,
        field: "input_images"
    },
    flux_2_dev_fields()
);
apiframe_model!(
    flux_2_flex,
    FLUX_2_FLEX,
    "apiframe-flux-2-flex",
    "Flux 2 Flex",
    "flux-2-flex",
    Some("fluxParams"),
    Slot::List {
        max: 10,
        field: "input_images"
    },
    flux_2_flex_fields()
);
apiframe_model!(
    flux_2_max,
    FLUX_2_MAX,
    "apiframe-flux-2-max",
    "Flux 2 Max",
    "flux-2-max",
    Some("fluxParams"),
    Slot::List {
        max: 8,
        field: "input_images"
    },
    flux_2_max_fields()
);
apiframe_model!(
    seedream_3,
    SEEDREAM_3,
    "apiframe-seedream-3",
    "Seedream 3",
    "seedream-3",
    Some("seedreamParams"),
    Slot::None,
    seedream_3_fields()
);
apiframe_model!(
    seedream_3_1,
    SEEDREAM_3_1,
    "apiframe-seedream-3.1",
    "Seedream 3.1",
    "seedream-3.1",
    Some("seedreamParams"),
    Slot::None,
    seedream_3_1_fields()
);
apiframe_model!(
    seedream_4,
    SEEDREAM_4,
    "apiframe-seedream-4",
    "Seedream 4",
    "seedream-4",
    Some("seedreamParams"),
    Slot::List {
        max: 10,
        field: "image_input"
    },
    seedream_4_fields()
);
apiframe_model!(
    seedream_4_5,
    SEEDREAM_4_5,
    "apiframe-seedream-4.5",
    "Seedream 4.5",
    "seedream-4.5",
    Some("seedreamParams"),
    Slot::List {
        max: 14,
        field: "image_input"
    },
    seedream_4_5_fields()
);
apiframe_model!(
    seedream_5_lite,
    SEEDREAM_5_LITE,
    "apiframe-seedream-5-lite",
    "Seedream 5 Lite",
    "seedream-5-lite",
    Some("seedreamParams"),
    Slot::List {
        max: 14,
        field: "image_input"
    },
    seedream_5_lite_fields()
);
apiframe_model!(
    seedream_5_pro,
    SEEDREAM_5_PRO,
    "apiframe-seedream-5-pro",
    "Seedream 5.0 Pro",
    "seedream-5-pro",
    Some("seedreamParams"),
    Slot::List {
        max: 10,
        field: "image_input"
    },
    seedream_5_pro_fields()
);
apiframe_model!(
    ideogram_v2,
    IDEOGRAM_V2,
    "apiframe-ideogram-v2",
    "Ideogram V2",
    "ideogram-v2",
    Some("ideogramParams"),
    Slot::None,
    ideogram_v2_fields()
);
apiframe_model!(
    ideogram_v2_turbo,
    IDEOGRAM_V2_TURBO,
    "apiframe-ideogram-v2-turbo",
    "Ideogram V2 Turbo",
    "ideogram-v2-turbo",
    Some("ideogramParams"),
    Slot::None,
    ideogram_v2_fields()
);
apiframe_model!(
    ideogram_v2a,
    IDEOGRAM_V2A,
    "apiframe-ideogram-v2a",
    "Ideogram V2a",
    "ideogram-v2a",
    Some("ideogramParams"),
    Slot::None,
    ideogram_v2_fields()
);
apiframe_model!(
    ideogram_v2a_turbo,
    IDEOGRAM_V2A_TURBO,
    "apiframe-ideogram-v2a-turbo",
    "Ideogram V2a Turbo",
    "ideogram-v2a-turbo",
    Some("ideogramParams"),
    Slot::None,
    ideogram_v2_fields()
);
apiframe_model!(
    ideogram_v3_balanced,
    IDEOGRAM_V3_BALANCED,
    "apiframe-ideogram-v3-balanced",
    "Ideogram V3 Balanced",
    "ideogram-v3-balanced",
    Some("ideogramParams"),
    Slot::List {
        max: 3,
        field: "style_reference_images"
    },
    ideogram_v3_fields()
);
apiframe_model!(
    ideogram_v3_turbo,
    IDEOGRAM_V3_TURBO,
    "apiframe-ideogram-v3-turbo",
    "Ideogram V3 Turbo",
    "ideogram-v3-turbo",
    Some("ideogramParams"),
    Slot::List {
        max: 3,
        field: "style_reference_images"
    },
    ideogram_v3_fields()
);
apiframe_model!(
    ideogram_v3_quality,
    IDEOGRAM_V3_QUALITY,
    "apiframe-ideogram-v3-quality",
    "Ideogram V3 Quality",
    "ideogram-v3-quality",
    Some("ideogramParams"),
    Slot::List {
        max: 3,
        field: "style_reference_images"
    },
    ideogram_v3_fields()
);
apiframe_model!(
    ideogram_v4_balanced,
    IDEOGRAM_V4_BALANCED,
    "apiframe-ideogram-v4-balanced",
    "Ideogram V4 Balanced",
    "ideogram-v4-balanced",
    Some("ideogramParams"),
    Slot::List {
        max: 3,
        field: "style_reference_images"
    },
    ideogram_v3_fields()
);
apiframe_model!(
    ideogram_v4_turbo,
    IDEOGRAM_V4_TURBO,
    "apiframe-ideogram-v4-turbo",
    "Ideogram V4 Turbo",
    "ideogram-v4-turbo",
    Some("ideogramParams"),
    Slot::List {
        max: 3,
        field: "style_reference_images"
    },
    ideogram_v3_fields()
);
apiframe_model!(
    ideogram_v4_quality,
    IDEOGRAM_V4_QUALITY,
    "apiframe-ideogram-v4-quality",
    "Ideogram V4 Quality",
    "ideogram-v4-quality",
    Some("ideogramParams"),
    Slot::List {
        max: 3,
        field: "style_reference_images"
    },
    ideogram_v3_fields()
);
apiframe_model!(
    ideogram_character,
    IDEOGRAM_CHARACTER,
    "apiframe-ideogram-character",
    "Ideogram Character",
    "ideogram-character",
    Some("ideogramParams"),
    Slot::Single {
        field: "character_reference_image"
    },
    ideogram_character_fields()
);
apiframe_model!(
    stable_diffusion_3_5_large,
    STABLE_DIFFUSION_3_5_LARGE,
    "apiframe-stable-diffusion-3.5-large",
    "Stable Diffusion 3.5 Large",
    "stable-diffusion-3.5-large",
    Some("stableDiffusionParams"),
    Slot::Single { field: "image" },
    sd35_large_fields()
);
apiframe_model!(
    stable_diffusion_3_5_large_turbo,
    STABLE_DIFFUSION_3_5_LARGE_TURBO,
    "apiframe-stable-diffusion-3.5-large-turbo",
    "Stable Diffusion 3.5 Large Turbo",
    "stable-diffusion-3.5-large-turbo",
    Some("stableDiffusionParams"),
    Slot::Single { field: "image" },
    sd35_turbo_fields()
);
apiframe_model!(
    stable_diffusion_3_5_medium,
    STABLE_DIFFUSION_3_5_MEDIUM,
    "apiframe-stable-diffusion-3.5-medium",
    "Stable Diffusion 3.5 Medium",
    "stable-diffusion-3.5-medium",
    Some("stableDiffusionParams"),
    Slot::Single { field: "image" },
    sd35_large_fields()
);
apiframe_model!(
    stable_diffusion_3,
    STABLE_DIFFUSION_3,
    "apiframe-stable-diffusion-3",
    "Stable Diffusion 3",
    "stable-diffusion-3",
    Some("stableDiffusionParams"),
    Slot::Single { field: "image" },
    sd3_fields()
);
apiframe_model!(
    imagen_3,
    IMAGEN_3,
    "apiframe-imagen-3",
    "Imagen 3",
    "imagen-3",
    Some("imagenParams"),
    Slot::None,
    imagen_3_fields()
);
apiframe_model!(
    imagen_3_fast,
    IMAGEN_3_FAST,
    "apiframe-imagen-3-fast",
    "Imagen 3 Fast",
    "imagen-3-fast",
    Some("imagenParams"),
    Slot::None,
    imagen_3_fields()
);
apiframe_model!(
    imagen_4,
    IMAGEN_4,
    "apiframe-imagen-4",
    "Imagen 4",
    "imagen-4",
    Some("imagenParams"),
    Slot::None,
    imagen_4_fields()
);
apiframe_model!(
    imagen_4_ultra,
    IMAGEN_4_ULTRA,
    "apiframe-imagen-4-ultra",
    "Imagen 4 Ultra",
    "imagen-4-ultra",
    Some("imagenParams"),
    Slot::None,
    imagen_4_fields()
);
apiframe_model!(
    imagen_4_fast,
    IMAGEN_4_FAST,
    "apiframe-imagen-4-fast",
    "Imagen 4 Fast",
    "imagen-4-fast",
    Some("imagenParams"),
    Slot::None,
    imagen_3_fields()
);
apiframe_model!(
    dall_e_3,
    DALL_E_3,
    "apiframe-dall-e-3",
    "DALL-E 3",
    "dall-e-3",
    Some("dalleParams"),
    Slot::None,
    dalle_3_fields()
);
apiframe_model!(
    dall_e_2,
    DALL_E_2,
    "apiframe-dall-e-2",
    "DALL-E 2",
    "dall-e-2",
    None,
    Slot::None,
    Vec::new()
);
apiframe_model!(
    gpt_image_1_5,
    GPT_IMAGE_1_5,
    "apiframe-gpt-image-1.5",
    "GPT Image 1.5",
    "gpt-image-1.5",
    Some("gptImageParams"),
    Slot::List {
        max: 10,
        field: "input_images"
    },
    gpt_image_fields()
);
apiframe_model!(
    kling_image,
    KLING_IMAGE,
    "apiframe-kling-image",
    "Kling Image",
    "kling-image",
    Some("klingImageParams"),
    Slot::Single { field: "image" },
    kling_fields()
);
apiframe_model!(
    qwen_image,
    QWEN_IMAGE,
    "apiframe-qwen-image",
    "Qwen Image",
    "qwen-image",
    Some("qwenImageParams"),
    Slot::Single { field: "image_url" },
    qwen_image_fields()
);
apiframe_model!(
    qwen_image_2,
    QWEN_IMAGE_2,
    "apiframe-qwen-image-2",
    "Qwen Image 2",
    "qwen-image-2",
    Some("qwenImage2Params"),
    Slot::Single { field: "image" },
    qwen_image_2_fields()
);
apiframe_model!(
    qwen_image_2_pro,
    QWEN_IMAGE_2_PRO,
    "apiframe-qwen-image-2-pro",
    "Qwen Image 2 Pro",
    "qwen-image-2-pro",
    Some("qwenImage2Params"),
    Slot::Single { field: "image" },
    qwen_image_2_fields()
);
apiframe_model!(
    wan_image_2_6,
    WAN_IMAGE_2_6,
    "apiframe-wan-image-2.6",
    "Wan Image 2.6",
    "wan-image-2.6",
    Some("wanImageParams"),
    Slot::Single { field: "image" },
    wan_fields()
);
apiframe_model!(
    wan_image_2_7,
    WAN_IMAGE_2_7,
    "apiframe-wan-image-2.7",
    "Wan Image 2.7",
    "wan-image-2.7",
    Some("wanImage27Params"),
    Slot::Single { field: "image" },
    wan_27_fields()
);
apiframe_model!(
    wan_image_2_7_pro,
    WAN_IMAGE_2_7_PRO,
    "apiframe-wan-image-2.7-pro",
    "Wan Image 2.7 Pro",
    "wan-image-2.7-pro",
    Some("wanImage27Params"),
    Slot::Single { field: "image" },
    wan_27_fields()
);
apiframe_model!(
    grok_image,
    GROK_IMAGE,
    "apiframe-grok-image",
    "Grok Image 2",
    "grok-image",
    Some("grokImageParams"),
    Slot::Single { field: "image" },
    grok_fields()
);
apiframe_model!(
    grok_imagine_image,
    GROK_IMAGINE_IMAGE,
    "apiframe-grok-imagine-image",
    "Grok Image Fast",
    "grok-imagine-image",
    Some("grokImagineParams"),
    Slot::Single { field: "image" },
    grok_fast_fields()
);
apiframe_model!(
    reve_create,
    REVE_CREATE,
    "apiframe-reve-create",
    "Reve Create",
    "reve-create",
    Some("reveParams"),
    Slot::None,
    reve_fields()
);

/// Every registered Apiframe descriptor, in `docs/api/apiframe.create.md` table order.
#[must_use]
pub fn descriptors() -> &'static [&'static Descriptor] {
    static ALL: LazyLock<Vec<&'static Descriptor>> = LazyLock::new(|| {
        vec![
            &MIDJOURNEY,
            &NANO_BANANA,
            &NANO_BANANA_PRO,
            &NANO_BANANA_2,
            &NANO_BANANA_2_LITE,
            &FLUX_11_PRO,
            &FLUX_11_PRO_ULTRA,
            &FLUX_2_PRO,
            &FLUX_2_DEV,
            &FLUX_2_FLEX,
            &FLUX_2_MAX,
            &SEEDREAM_3,
            &SEEDREAM_3_1,
            &SEEDREAM_4,
            &SEEDREAM_4_5,
            &SEEDREAM_5_LITE,
            &SEEDREAM_5_PRO,
            &IDEOGRAM_V2,
            &IDEOGRAM_V2_TURBO,
            &IDEOGRAM_V2A,
            &IDEOGRAM_V2A_TURBO,
            &IDEOGRAM_V3_BALANCED,
            &IDEOGRAM_V3_TURBO,
            &IDEOGRAM_V3_QUALITY,
            &IDEOGRAM_V4_BALANCED,
            &IDEOGRAM_V4_TURBO,
            &IDEOGRAM_V4_QUALITY,
            &IDEOGRAM_CHARACTER,
            &STABLE_DIFFUSION_3_5_LARGE,
            &STABLE_DIFFUSION_3_5_LARGE_TURBO,
            &STABLE_DIFFUSION_3_5_MEDIUM,
            &STABLE_DIFFUSION_3,
            &IMAGEN_3,
            &IMAGEN_3_FAST,
            &IMAGEN_4,
            &IMAGEN_4_ULTRA,
            &IMAGEN_4_FAST,
            &DALL_E_3,
            &DALL_E_2,
            &GPT_IMAGE_1_5,
            &KLING_IMAGE,
            &QWEN_IMAGE,
            &QWEN_IMAGE_2,
            &QWEN_IMAGE_2_PRO,
            &WAN_IMAGE_2_6,
            &WAN_IMAGE_2_7,
            &WAN_IMAGE_2_7_PRO,
            &GROK_IMAGE,
            &GROK_IMAGINE_IMAGE,
            &REVE_CREATE,
        ]
    });
    ALL.as_slice()
}

/// The Apiframe provider that owns a registered model id.
#[must_use]
pub fn resolve(model_id: &str) -> Option<Apiframe> {
    descriptors()
        .iter()
        .copied()
        .find(|descriptor| descriptor.spec.id == model_id)
        .map(|descriptor| Apiframe { descriptor })
}

impl crate::providers::Provider for Apiframe {
    fn spec(&self) -> &ModelSpec {
        &self.descriptor.spec
    }

    fn validate_for_submit(
        &self,
        scene: &Scene,
        values: &BTreeMap<String, Value>,
        image_count: usize,
    ) -> Vec<FieldError> {
        validate(self.descriptor, scene, values, image_count)
    }

    fn reference_steps(&self, images: &[EncodedImage]) -> Vec<ReferenceStep> {
        upload_steps(images, self.spec().capabilities.image_slots.max())
    }

    fn parse_upload(&self, response: &HttpResponse) -> Result<ImageInput, ProviderError> {
        parse_upload(response)
    }

    fn build_submit(&self, request: &JobRequest, images: &[ImageInput]) -> HttpRequest {
        build_submit(self.descriptor, request, images)
    }

    fn poll_request(&self, submitted: &Submitted) -> HttpRequest {
        poll_request(submitted)
    }

    fn parse_submit(&self, response: &HttpResponse) -> Result<Submitted, ProviderError> {
        parse_submit(response)
    }

    fn parse_poll(&self, response: &HttpResponse) -> PollOutcome {
        parse_poll(self.descriptor, response)
    }

    fn poll_policy(&self) -> PollPolicy {
        poll_policy()
    }
}

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
    if scene.prose.trim().is_empty() {
        errors.push(FieldError {
            key: "prompt".into(),
            message: "Prompt is empty".into(),
            approved_max: None,
        });
    } else if scene.prose.chars().count() > MAX_PROMPT_CHARS {
        errors.push(FieldError {
            key: "prompt".into(),
            message: format!("Prompt must be at most {MAX_PROMPT_CHARS} characters"),
            approved_max: None,
        });
    }
    if descriptor.wire_id == "qwen-image" {
        let task_type = values
            .get("task_type")
            .and_then(Value::as_str)
            .unwrap_or("text-to-image");
        if task_type != "text-to-image" && image_count == 0 {
            errors.push(FieldError {
                key: "images".into(),
                message: "Add at least one image.".into(),
                approved_max: None,
            });
        }
    }
    errors
}

fn upload_steps(images: &[EncodedImage], max: usize) -> Vec<ReferenceStep> {
    images
        .iter()
        .take(max)
        .map(|image| ReferenceStep::Upload {
            request: upload_request(image),
            content_hash: image.content_hash.clone(),
        })
        .collect()
}

fn upload_request(image: &EncodedImage) -> HttpRequest {
    let extension = match image.media_type.as_str() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "bin",
    };
    HttpRequest {
        method: Method::Post,
        url: UPLOAD_URL.into(),
        headers: Vec::new(),
        auth: Some(ProviderId::Apiframe),
        body: Body::Multipart {
            parts: vec![Part {
                name: "file".into(),
                filename: Some(format!("image.{extension}")),
                content_type: image.media_type.clone(),
                bytes: image.bytes.clone(),
            }],
        },
        max_body_bytes: 1024 * 1024,
        redact_query: false,
    }
}

fn parse_upload(response: &HttpResponse) -> Result<ImageInput, ProviderError> {
    if response.status != 200 && response.status != 201 {
        return Err(classify_error(response));
    }
    let value = serde_json::from_slice::<Value>(&response.body)
        .map_err(|_| ProviderError::Protocol("upload response was not json".into()))?;
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::Protocol("unexpected upload host".into()))?;
    if !ProviderId::Apiframe.allows_url(url) {
        return Err(ProviderError::Protocol("unexpected upload host".into()));
    }
    Ok(ImageInput::Url(url.to_owned()))
}

fn collect_params(
    schema: &ParamSchema,
    values: &BTreeMap<String, Value>,
    params: &mut Map<String, Value>,
) {
    for field in &schema.fields {
        let Some(value) = values.get(&field.key) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        if value.as_str().is_some_and(str::is_empty) {
            continue;
        }
        if field.key == "seed" && value.as_i64() == Some(0) {
            continue;
        }
        params.insert(field.key.clone(), value.clone());
    }
}

fn build_submit(
    descriptor: &Descriptor,
    request: &JobRequest,
    images: &[ImageInput],
) -> HttpRequest {
    let mut params = Map::new();
    collect_params(&descriptor.spec.schema, &request.params, &mut params);
    match descriptor.slot {
        Slot::None => {}
        Slot::Single { field } => {
            if let Some(input) = images.first() {
                params.insert(
                    (*field).to_owned(),
                    Value::String(input.as_str().to_owned()),
                );
            }
        }
        Slot::List { max, field } => {
            let urls: Vec<Value> = images
                .iter()
                .take(max)
                .map(|input| Value::String(input.as_str().to_owned()))
                .collect();
            if !urls.is_empty() {
                params.insert((*field).to_owned(), Value::Array(urls));
            }
        }
    }
    if descriptor.wire_id == "qwen-image"
        && !images.is_empty()
        && params.get("task_type").and_then(Value::as_str) == Some("text-to-image")
    {
        params.insert("task_type".into(), Value::String("image-to-image".into()));
    }
    let mut body = Map::new();
    body.insert(
        "prompt".into(),
        Value::String(request.compiled_prompt.clone()),
    );
    body.insert("model".into(), Value::String(descriptor.wire_id.to_owned()));
    if let Some(key) = descriptor.params_key
        && !params.is_empty()
    {
        body.insert((*key).to_owned(), Value::Object(params));
    }
    HttpRequest {
        method: Method::Post,
        url: GENERATE_URL.into(),
        headers: vec![HttpHeader {
            name: "Content-Type".into(),
            value: "application/json".into(),
            sensitive: false,
        }],
        auth: Some(ProviderId::Apiframe),
        body: Body::Json(Value::Object(body)),
        max_body_bytes: 1024 * 1024,
        redact_query: false,
    }
}

#[must_use]
pub fn poll_request(submitted: &Submitted) -> HttpRequest {
    HttpRequest {
        method: Method::Get,
        url: submitted.poll_url.to_string(),
        headers: Vec::new(),
        auth: Some(ProviderId::Apiframe),
        body: Body::Empty,
        max_body_bytes: 2 * 1024 * 1024,
        redact_query: false,
    }
}

#[must_use]
pub fn poll_policy() -> PollPolicy {
    PollPolicy {
        initial: Duration::from_secs(2),
        factor: 1.25,
        cap: Duration::from_secs(3),
        deadline: Duration::from_secs(600),
    }
}

fn parse_submit(response: &HttpResponse) -> Result<Submitted, ProviderError> {
    if response.status != 200 && response.status != 202 {
        return Err(classify_error(response));
    }
    let value = serde_json::from_slice::<Value>(&response.body)
        .map_err(|_| ProviderError::Protocol("submit response was not json".into()))?;
    let id = value
        .get("jobId")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::Protocol("submit response had no jobId".into()))?;
    let job_id = JobId::parse(id)
        .map_err(|_| ProviderError::Protocol("submit response had no jobId".into()))?;
    let poll_url = url::Url::parse(&format!("{JOBS_URL}/{}", job_id.as_str()))
        .map_err(|_| ProviderError::Protocol("poll url was not valid".into()))?;
    Ok(Submitted {
        job_id,
        poll_url,
        quote: None,
    })
}

fn parse_poll(descriptor: &Descriptor, response: &HttpResponse) -> PollOutcome {
    if response.status != 200 {
        return PollOutcome::Failed(classify_error(response));
    }
    let Ok(value) = serde_json::from_slice::<Value>(&response.body) else {
        return PollOutcome::Failed(ProviderError::Protocol("poll body was not json".into()));
    };
    let Some(status) = value.get("status").and_then(Value::as_str) else {
        return PollOutcome::Failed(ProviderError::Protocol("poll body had no status".into()));
    };
    match status {
        "QUEUED" | "PROCESSING" => PollOutcome::InFlight {
            status: status.to_owned(),
            progress: value
                .get("progress")
                .and_then(Value::as_f64)
                .map(|progress| ((progress / 100.0).clamp(0.0, 1.0)) as f32),
        },
        "FAILED" => PollOutcome::Failed(failed_error(&value)),
        "COMPLETED" => completed_outcome(descriptor, &value),
        other => PollOutcome::Failed(ProviderError::Protocol(format!(
            "unexpected status {other}"
        ))),
    }
}

fn failed_error(value: &Value) -> ProviderError {
    let error = value.get("error").and_then(Value::as_str).unwrap_or("");
    let lowered = error.to_ascii_lowercase();
    if lowered.contains("content policy") || lowered.contains("moderation") {
        ProviderError::Blocked
    } else if error.is_empty() {
        ProviderError::Protocol("job failed".into())
    } else {
        ProviderError::Protocol(error.to_owned())
    }
}

fn image_url_at(result: &Value, index: usize) -> Option<url::Url> {
    allowed_url(result.get("images")?.get(index)?.as_str())
}

fn grid_url_of(result: &Value) -> Option<url::Url> {
    allowed_url(result.get("gridUrl")?.as_str())
}

fn allowed_url(raw: Option<&str>) -> Option<url::Url> {
    let raw = raw?;
    if !ProviderId::Apiframe.allows_url(raw) {
        return None;
    }
    raw.parse().ok()
}

fn completed_outcome(descriptor: &Descriptor, value: &Value) -> PollOutcome {
    let Some(result) = value.get("result") else {
        return PollOutcome::Failed(ProviderError::Protocol(
            "completed poll had no result".into(),
        ));
    };
    let settled_cost = value.get("creditCost").and_then(Value::as_f64);
    let grid = grid_url_of(result);
    if descriptor.wire_id == "midjourney" || grid.is_some() {
        let Some(grid) = grid else {
            return PollOutcome::Failed(ProviderError::Protocol(
                "completed Midjourney poll had no gridUrl".into(),
            ));
        };
        let Some(quadrants) = (0..4)
            .map(|index| image_url_at(result, index))
            .collect::<Option<Vec<_>>>()
            .and_then(|urls| <[url::Url; 4]>::try_from(urls).ok())
        else {
            return PollOutcome::Failed(ProviderError::Protocol(
                "completed Midjourney poll needs four valid image urls".into(),
            ));
        };
        PollOutcome::Ready {
            url: grid,
            settled_cost,
            prompt: None,
            duration: None,
            grid_urls: Some(quadrants),
        }
    } else {
        let Some(url) = image_url_at(result, 0) else {
            return PollOutcome::Failed(ProviderError::Protocol(
                "completed poll had no result url".into(),
            ));
        };
        PollOutcome::Ready {
            url,
            settled_cost,
            prompt: None,
            duration: None,
            grid_urls: None,
        }
    }
}

fn classify_error(response: &HttpResponse) -> ProviderError {
    match response.status {
        401 | 403 => ProviderError::Auth,
        402 => ProviderError::OutOfCredits { hint: None },
        404 => ProviderError::ModelNotFound,
        429 => ProviderError::RateLimited { retry_after: None },
        400 | 422 => ProviderError::Validation {
            message: cap_text(&error_text(&response.body)),
            field_errors: Vec::new(),
        },
        status => ProviderError::Http {
            status,
            body: cap_text(&String::from_utf8_lossy(&response.body)),
        },
    }
}

fn error_text(body: &[u8]) -> String {
    let value = serde_json::from_slice::<Value>(body).unwrap_or(Value::Null);
    match value.get("error") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Object(map)) => map
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("validation failed")
            .to_owned(),
        _ => String::from_utf8_lossy(body).trim().to_owned(),
    }
}

fn cap_text(text: &str) -> String {
    crate::text::cap_text(text, STORED_CAP)
}

#[cfg(test)]
mod tests {
    use crate::error::ProviderError;
    use crate::http::{Body, HttpResponse, Method};
    use crate::job::{EncodedImage, JobRequest};
    use crate::model_spec::{AuthScheme, ImageSlots, ProviderId, SizeControl};
    use crate::providers::{
        ImageInput, ModelRegistry, PollOutcome, Provider, ReferenceStep, resolve_model,
    };
    use crate::scene::Scene;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    const IDS: [&str; 50] = [
        "apiframe-midjourney",
        "apiframe-nano-banana",
        "apiframe-nano-banana-pro",
        "apiframe-nano-banana-2",
        "apiframe-nano-banana-2-lite",
        "apiframe-flux-1.1-pro",
        "apiframe-flux-1.1-pro-ultra",
        "apiframe-flux-2-pro",
        "apiframe-flux-2-dev",
        "apiframe-flux-2-flex",
        "apiframe-flux-2-max",
        "apiframe-seedream-3",
        "apiframe-seedream-3.1",
        "apiframe-seedream-4",
        "apiframe-seedream-4.5",
        "apiframe-seedream-5-lite",
        "apiframe-seedream-5-pro",
        "apiframe-ideogram-v2",
        "apiframe-ideogram-v2-turbo",
        "apiframe-ideogram-v2a",
        "apiframe-ideogram-v2a-turbo",
        "apiframe-ideogram-v3-balanced",
        "apiframe-ideogram-v3-turbo",
        "apiframe-ideogram-v3-quality",
        "apiframe-ideogram-v4-balanced",
        "apiframe-ideogram-v4-turbo",
        "apiframe-ideogram-v4-quality",
        "apiframe-ideogram-character",
        "apiframe-stable-diffusion-3.5-large",
        "apiframe-stable-diffusion-3.5-large-turbo",
        "apiframe-stable-diffusion-3.5-medium",
        "apiframe-stable-diffusion-3",
        "apiframe-imagen-3",
        "apiframe-imagen-3-fast",
        "apiframe-imagen-4",
        "apiframe-imagen-4-ultra",
        "apiframe-imagen-4-fast",
        "apiframe-dall-e-3",
        "apiframe-dall-e-2",
        "apiframe-gpt-image-1.5",
        "apiframe-kling-image",
        "apiframe-qwen-image",
        "apiframe-qwen-image-2",
        "apiframe-qwen-image-2-pro",
        "apiframe-wan-image-2.6",
        "apiframe-wan-image-2.7",
        "apiframe-wan-image-2.7-pro",
        "apiframe-grok-image",
        "apiframe-grok-imagine-image",
        "apiframe-reve-create",
    ];

    fn provider(id: &str) -> Box<dyn Provider> {
        resolve_model(id).unwrap_or_else(|| panic!("missing {id}"))
    }

    fn spec_of(id: &str) -> &'static crate::model_spec::ModelSpec {
        ModelRegistry::specs()
            .iter()
            .copied()
            .find(|spec| spec.id == id)
            .unwrap_or_else(|| panic!("missing {id}"))
    }

    fn request(model: &str, params: BTreeMap<String, Value>) -> JobRequest {
        JobRequest {
            model_id: model.into(),
            compiled_prompt: "a lighthouse".into(),
            params,
            images: Vec::new(),
            box_snapshot: Vec::new(),
            created_at: "2026-10-25T00:00:00Z".into(),
            conversation: None,
        }
    }

    fn defaults(model: &str) -> BTreeMap<String, Value> {
        spec_of(model).schema.defaults()
    }

    fn posted(model: &str, params: BTreeMap<String, Value>, urls: &[&str]) -> Value {
        let provider = provider(model);
        let inputs: Vec<ImageInput> = urls
            .iter()
            .map(|url| ImageInput::Url((*url).to_owned()))
            .collect();
        let http = provider.build_submit(&request(model, params), &inputs);
        assert_eq!(http.method, Method::Post);
        assert_eq!(http.url, super::GENERATE_URL);
        assert_eq!(http.auth, Some(ProviderId::Apiframe));
        assert!(
            http.headers
                .iter()
                .any(|header| header.name == "Content-Type" && header.value == "application/json")
        );
        let Body::Json(body) = http.body else {
            panic!("expected json");
        };
        body
    }

    fn body_of(model: &str, urls: &[&str]) -> Value {
        posted(model, defaults(model), urls)
    }

    fn encoded(seed: u8) -> EncodedImage {
        EncodedImage {
            media_type: "image/png".into(),
            bytes: vec![seed; 8],
            width: 16,
            height: 16,
            content_hash: format!("h{seed}"),
        }
    }

    fn scene_with(prose: &str) -> Scene {
        let mut scene = Scene::empty();
        scene.prose = prose.into();
        scene
    }

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn registry_lists_all_fifty_apiframe_models_in_table_order() {
        let ids: Vec<_> = ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.provider == ProviderId::Apiframe)
            .map(|spec| spec.id.as_str())
            .collect();
        assert_eq!(ids, IDS);
        for id in IDS {
            let spec = spec_of(id);
            assert_eq!(spec.provider, ProviderId::Apiframe, "{id}");
            assert!(
                matches!(spec.auth, AuthScheme::HeaderKey { ref header } if header == "X-API-Key"),
                "{id}"
            );
            assert_eq!(spec.endpoint, super::GENERATE_URL, "{id}");
            assert!(!spec.beta, "{id}");
            assert!(!spec.capabilities.supports_boxes, "{id}");
            assert_eq!(
                spec.capabilities.size_control,
                SizeControl::AspectRatioAndResolution,
                "{id}"
            );
            let resolved = resolve_model(id).unwrap_or_else(|| panic!("unresolved {id}"));
            assert_eq!(resolved.spec().id, id, "{id}");
        }
        assert!(ModelRegistry::providers().contains(&ProviderId::Apiframe));
    }

    #[test]
    fn image_slot_caps_follow_the_model_table() {
        let expectations: &[(&str, usize)] = &[
            ("apiframe-midjourney", 0),
            ("apiframe-nano-banana", 14),
            ("apiframe-nano-banana-pro", 14),
            ("apiframe-nano-banana-2", 14),
            ("apiframe-nano-banana-2-lite", 14),
            ("apiframe-flux-1.1-pro", 1),
            ("apiframe-flux-1.1-pro-ultra", 1),
            ("apiframe-flux-2-pro", 8),
            ("apiframe-flux-2-dev", 5),
            ("apiframe-flux-2-flex", 10),
            ("apiframe-flux-2-max", 8),
            ("apiframe-seedream-3", 0),
            ("apiframe-seedream-3.1", 0),
            ("apiframe-seedream-4", 10),
            ("apiframe-seedream-4.5", 14),
            ("apiframe-seedream-5-lite", 14),
            ("apiframe-seedream-5-pro", 10),
            ("apiframe-ideogram-v2", 0),
            ("apiframe-ideogram-v2-turbo", 0),
            ("apiframe-ideogram-v2a", 0),
            ("apiframe-ideogram-v2a-turbo", 0),
            ("apiframe-ideogram-v3-balanced", 3),
            ("apiframe-ideogram-v3-turbo", 3),
            ("apiframe-ideogram-v3-quality", 3),
            ("apiframe-ideogram-v4-balanced", 3),
            ("apiframe-ideogram-v4-turbo", 3),
            ("apiframe-ideogram-v4-quality", 3),
            ("apiframe-ideogram-character", 1),
            ("apiframe-stable-diffusion-3.5-large", 1),
            ("apiframe-stable-diffusion-3.5-large-turbo", 1),
            ("apiframe-stable-diffusion-3.5-medium", 1),
            ("apiframe-stable-diffusion-3", 1),
            ("apiframe-imagen-3", 0),
            ("apiframe-imagen-3-fast", 0),
            ("apiframe-imagen-4", 0),
            ("apiframe-imagen-4-ultra", 0),
            ("apiframe-imagen-4-fast", 0),
            ("apiframe-dall-e-3", 0),
            ("apiframe-dall-e-2", 0),
            ("apiframe-gpt-image-1.5", 10),
            ("apiframe-kling-image", 1),
            ("apiframe-qwen-image", 1),
            ("apiframe-qwen-image-2", 1),
            ("apiframe-qwen-image-2-pro", 1),
            ("apiframe-wan-image-2.6", 1),
            ("apiframe-wan-image-2.7", 1),
            ("apiframe-wan-image-2.7-pro", 1),
            ("apiframe-grok-image", 1),
            ("apiframe-grok-imagine-image", 1),
            ("apiframe-reve-create", 0),
        ];
        for (id, max) in expectations {
            assert_eq!(spec_of(id).capabilities.image_slots.max(), *max, "{id}");
        }
        match &spec_of("apiframe-flux-2-pro").capabilities.image_slots {
            ImageSlots::List { key, .. } => assert_eq!(key, "input_images"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_model_posts_its_wire_model_and_family_wrapper() {
        let wrappers: &[(&str, Option<&str>)] = &[
            ("apiframe-midjourney", Some("midjourneyParams")),
            ("apiframe-nano-banana", Some("nanoBananaParams")),
            ("apiframe-nano-banana-pro", Some("nanoBananaParams")),
            ("apiframe-nano-banana-2", Some("nanoBananaParams")),
            ("apiframe-nano-banana-2-lite", Some("nanoBananaParams")),
            ("apiframe-flux-1.1-pro", Some("fluxParams")),
            ("apiframe-flux-1.1-pro-ultra", Some("fluxParams")),
            ("apiframe-flux-2-pro", Some("fluxParams")),
            ("apiframe-flux-2-dev", Some("fluxParams")),
            ("apiframe-flux-2-flex", Some("fluxParams")),
            ("apiframe-flux-2-max", Some("fluxParams")),
            ("apiframe-seedream-3", Some("seedreamParams")),
            ("apiframe-seedream-3.1", Some("seedreamParams")),
            ("apiframe-seedream-4", Some("seedreamParams")),
            ("apiframe-seedream-4.5", Some("seedreamParams")),
            ("apiframe-seedream-5-lite", Some("seedreamParams")),
            ("apiframe-seedream-5-pro", Some("seedreamParams")),
            ("apiframe-ideogram-v2", Some("ideogramParams")),
            ("apiframe-ideogram-v2-turbo", Some("ideogramParams")),
            ("apiframe-ideogram-v2a", Some("ideogramParams")),
            ("apiframe-ideogram-v2a-turbo", Some("ideogramParams")),
            ("apiframe-ideogram-v3-balanced", Some("ideogramParams")),
            ("apiframe-ideogram-v3-turbo", Some("ideogramParams")),
            ("apiframe-ideogram-v3-quality", Some("ideogramParams")),
            ("apiframe-ideogram-v4-balanced", Some("ideogramParams")),
            ("apiframe-ideogram-v4-turbo", Some("ideogramParams")),
            ("apiframe-ideogram-v4-quality", Some("ideogramParams")),
            ("apiframe-ideogram-character", Some("ideogramParams")),
            (
                "apiframe-stable-diffusion-3.5-large",
                Some("stableDiffusionParams"),
            ),
            (
                "apiframe-stable-diffusion-3.5-large-turbo",
                Some("stableDiffusionParams"),
            ),
            (
                "apiframe-stable-diffusion-3.5-medium",
                Some("stableDiffusionParams"),
            ),
            ("apiframe-stable-diffusion-3", Some("stableDiffusionParams")),
            ("apiframe-imagen-3", Some("imagenParams")),
            ("apiframe-imagen-3-fast", Some("imagenParams")),
            ("apiframe-imagen-4", Some("imagenParams")),
            ("apiframe-imagen-4-ultra", Some("imagenParams")),
            ("apiframe-imagen-4-fast", Some("imagenParams")),
            ("apiframe-dall-e-3", Some("dalleParams")),
            ("apiframe-dall-e-2", None),
            ("apiframe-gpt-image-1.5", Some("gptImageParams")),
            ("apiframe-kling-image", Some("klingImageParams")),
            ("apiframe-qwen-image", Some("qwenImageParams")),
            ("apiframe-qwen-image-2", Some("qwenImage2Params")),
            ("apiframe-qwen-image-2-pro", Some("qwenImage2Params")),
            ("apiframe-wan-image-2.6", Some("wanImageParams")),
            ("apiframe-wan-image-2.7", Some("wanImage27Params")),
            ("apiframe-wan-image-2.7-pro", Some("wanImage27Params")),
            ("apiframe-grok-image", Some("grokImageParams")),
            ("apiframe-grok-imagine-image", Some("grokImagineParams")),
            ("apiframe-reve-create", Some("reveParams")),
        ];
        for (id, params_key) in wrappers {
            let body = body_of(id, &[]);
            let wire = id.strip_prefix("apiframe-").unwrap();
            assert_eq!(body["model"], wire, "{id}");
            assert_eq!(body["prompt"], "a lighthouse", "{id}");
            let keys: Vec<_> = body.as_object().unwrap().keys().collect();
            match params_key {
                Some(key) => {
                    assert!(body[key].is_object(), "{id}: missing {key}");
                    assert_eq!(keys.len(), 3, "{id}: {keys:?}");
                }
                None => assert_eq!(keys.len(), 2, "{id}: {keys:?}"),
            }
        }
    }

    #[test]
    fn each_family_wrapper_builds_its_signature_payload() {
        let checks: &[(&str, &str, Value)] = &[
            (
                "apiframe-midjourney",
                "midjourneyParams.aspect_ratio",
                json!("1:1"),
            ),
            (
                "apiframe-nano-banana",
                "nanoBananaParams.output_format",
                json!("png"),
            ),
            (
                "apiframe-nano-banana-pro",
                "nanoBananaParams.resolution",
                json!("2K"),
            ),
            (
                "apiframe-nano-banana-2",
                "nanoBananaParams.google_search",
                json!(false),
            ),
            (
                "apiframe-nano-banana-2-lite",
                "nanoBananaParams.image_search",
                json!(false),
            ),
            (
                "apiframe-flux-1.1-pro",
                "fluxParams.output_quality",
                json!(90),
            ),
            (
                "apiframe-flux-1.1-pro-ultra",
                "fluxParams.image_prompt_strength",
                json!(0.1),
            ),
            (
                "apiframe-flux-2-pro",
                "fluxParams.resolution",
                json!("1 MP"),
            ),
            ("apiframe-flux-2-dev", "fluxParams.go_fast", json!(true)),
            ("apiframe-flux-2-flex", "fluxParams.guidance", json!(4.5)),
            (
                "apiframe-flux-2-max",
                "fluxParams.prompt_upsampling",
                json!(false),
            ),
            (
                "apiframe-seedream-3",
                "seedreamParams.guidance_scale",
                json!(2.5),
            ),
            (
                "apiframe-seedream-3.1",
                "seedreamParams.use_pre_llm",
                json!(true),
            ),
            (
                "apiframe-seedream-4",
                "seedreamParams.enhance_prompt",
                json!(true),
            ),
            ("apiframe-seedream-4.5", "seedreamParams.size", json!("2K")),
            (
                "apiframe-seedream-5-lite",
                "seedreamParams.output_format",
                json!("png"),
            ),
            (
                "apiframe-seedream-5-pro",
                "seedreamParams.size",
                json!("2K"),
            ),
            (
                "apiframe-ideogram-v2",
                "ideogramParams.style_type",
                json!("Auto"),
            ),
            (
                "apiframe-ideogram-v3-balanced",
                "ideogramParams.magic_prompt_option",
                json!("Auto"),
            ),
            (
                "apiframe-ideogram-character",
                "ideogramParams.rendering_speed",
                json!("default"),
            ),
            (
                "apiframe-stable-diffusion-3.5-large",
                "stableDiffusionParams.cfg",
                json!(3.5),
            ),
            (
                "apiframe-stable-diffusion-3.5-medium",
                "stableDiffusionParams.steps",
                json!(28),
            ),
            (
                "apiframe-stable-diffusion-3",
                "stableDiffusionParams.steps",
                json!(28),
            ),
            (
                "apiframe-imagen-3",
                "imagenParams.output_format",
                json!("jpg"),
            ),
            ("apiframe-imagen-4", "imagenParams.image_size", json!("1K")),
            ("apiframe-dall-e-3", "dalleParams.style", json!("vivid")),
            (
                "apiframe-gpt-image-1.5",
                "gptImageParams.input_fidelity",
                json!("low"),
            ),
            (
                "apiframe-kling-image",
                "klingImageParams.model_name",
                json!("kling-v1"),
            ),
            (
                "apiframe-qwen-image",
                "qwenImageParams.task_type",
                json!("text-to-image"),
            ),
            (
                "apiframe-qwen-image-2",
                "qwenImage2Params.aspect_ratio",
                json!("1:1"),
            ),
            (
                "apiframe-wan-image-2.6",
                "wanImageParams.prompt_extend",
                json!(false),
            ),
            (
                "apiframe-wan-image-2.7",
                "wanImage27Params.thinking_mode",
                json!(true),
            ),
            (
                "apiframe-grok-image",
                "grokImageParams.resolution",
                json!("2k"),
            ),
            (
                "apiframe-grok-imagine-image",
                "grokImagineParams.aspect_ratio",
                json!("1:1"),
            ),
            (
                "apiframe-reve-create",
                "reveParams.version",
                json!("latest"),
            ),
        ];
        for (id, pointer, expected) in checks {
            let body = body_of(id, &[]);
            let (key, field) = pointer.split_once('.').unwrap();
            assert_eq!(body[key][field], *expected, "{id}: {pointer}");
        }
    }

    #[test]
    fn the_turbo_model_drops_the_negative_prompt_and_sends_fast_defaults() {
        let body = body_of("apiframe-stable-diffusion-3.5-large-turbo", &[]);
        assert_eq!(body["stableDiffusionParams"]["cfg"], json!(1.0));
        assert_eq!(body["stableDiffusionParams"]["steps"], 4);
        assert!(
            body["stableDiffusionParams"]
                .get("negative_prompt")
                .is_none()
        );
    }

    #[test]
    fn seeds_are_omitted_when_unset_or_zero_and_sent_otherwise() {
        let body = body_of("apiframe-flux-1.1-pro", &[]);
        assert!(body["fluxParams"].get("seed").is_none());
        let mut zero = defaults("apiframe-flux-1.1-pro");
        zero.insert("seed".into(), json!(0));
        let body = posted("apiframe-flux-1.1-pro", zero, &[]);
        assert!(body["fluxParams"].get("seed").is_none());
        let mut set = defaults("apiframe-flux-1.1-pro");
        set.insert("seed".into(), json!(7));
        let body = posted("apiframe-flux-1.1-pro", set, &[]);
        assert_eq!(body["fluxParams"]["seed"], 7);
    }

    #[test]
    fn reference_steps_upload_to_the_uploads_endpoint_and_clip_to_the_cap() {
        let images: Vec<_> = (0..4).map(encoded).collect();
        let steps = provider("apiframe-flux-2-pro").reference_steps(&images);
        assert_eq!(steps.len(), 4);
        for (step, image) in steps.iter().zip(&images) {
            let ReferenceStep::Upload {
                request,
                content_hash,
            } = step
            else {
                panic!("expected upload");
            };
            assert_eq!(request.method, Method::Post);
            assert_eq!(request.url, super::UPLOAD_URL);
            assert_eq!(request.auth, Some(ProviderId::Apiframe));
            let Body::Multipart { parts } = &request.body else {
                panic!("multipart");
            };
            assert_eq!(parts.len(), 1);
            assert_eq!(parts[0].name, "file");
            assert_eq!(parts[0].filename.as_deref(), Some("image.png"));
            assert_eq!(parts[0].content_type, "image/png");
            assert_eq!(content_hash, &image.content_hash);
        }
        assert_eq!(
            provider("apiframe-flux-1.1-pro")
                .reference_steps(&images)
                .len(),
            1
        );
        let jpeg = EncodedImage {
            media_type: "image/jpeg".into(),
            bytes: vec![1, 2, 3],
            width: 16,
            height: 16,
            content_hash: "j".into(),
        };
        let steps = provider("apiframe-nano-banana").reference_steps(&[jpeg, encoded(2)]);
        let ReferenceStep::Upload { request, .. } = &steps[0] else {
            panic!("expected upload");
        };
        assert_eq!(request.url, super::UPLOAD_URL);
        let Body::Multipart { parts } = &request.body else {
            panic!("multipart");
        };
        assert_eq!(parts[0].filename.as_deref(), Some("image.jpg"));
        assert_eq!(parts[0].content_type, "image/jpeg");
        assert!(
            provider("apiframe-dall-e-3")
                .reference_steps(&images)
                .is_empty()
        );
        assert!(
            provider("apiframe-midjourney")
                .reference_steps(&images)
                .is_empty()
        );
    }

    #[test]
    fn multi_reference_urls_reach_the_documented_list_field() {
        let urls = [
            "https://cdn2.apiframe.ai/uploads/one.png",
            "https://cdn2.apiframe.ai/uploads/two.png",
        ];
        let body = body_of("apiframe-flux-2-pro", &urls);
        let inputs = body["fluxParams"]["input_images"].as_array().unwrap();
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0], urls[0]);
        assert_eq!(inputs[1], urls[1]);
        let body = body_of("apiframe-dall-e-3", &urls);
        let keys: Vec<_> = body.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 3);
    }

    #[test]
    fn single_image_urls_reach_the_documented_field() {
        let urls = ["https://cdn2.apiframe.ai/uploads/one.png"];
        let cases: &[(&str, &str)] = &[
            ("apiframe-flux-1.1-pro", "fluxParams.image_prompt"),
            (
                "apiframe-ideogram-character",
                "ideogramParams.character_reference_image",
            ),
            ("apiframe-qwen-image", "qwenImageParams.image_url"),
            (
                "apiframe-stable-diffusion-3.5-large",
                "stableDiffusionParams.image",
            ),
            ("apiframe-stable-diffusion-3", "stableDiffusionParams.image"),
            ("apiframe-kling-image", "klingImageParams.image"),
            ("apiframe-qwen-image-2", "qwenImage2Params.image"),
            ("apiframe-qwen-image-2-pro", "qwenImage2Params.image"),
            ("apiframe-wan-image-2.6", "wanImageParams.image"),
            ("apiframe-wan-image-2.7", "wanImage27Params.image"),
            ("apiframe-wan-image-2.7-pro", "wanImage27Params.image"),
            ("apiframe-grok-image", "grokImageParams.image"),
            ("apiframe-grok-imagine-image", "grokImagineParams.image"),
        ];
        for (id, pointer) in cases {
            let body = body_of(id, &urls);
            let (key, field) = pointer.split_once('.').unwrap();
            assert_eq!(body[key][field], urls[0], "{id}: {pointer}");
        }
    }

    #[test]
    fn qwen_image_promotes_task_type_when_an_image_is_sent() {
        let urls = ["https://cdn2.apiframe.ai/uploads/one.png"];
        let body = body_of("apiframe-qwen-image", &urls);
        assert_eq!(body["qwenImageParams"]["task_type"], "image-to-image");
        assert_eq!(body["qwenImageParams"]["image_url"], urls[0]);
        let body = body_of("apiframe-qwen-image", &[]);
        assert_eq!(body["qwenImageParams"]["task_type"], "text-to-image");
        let mut edit = defaults("apiframe-qwen-image");
        edit.insert("task_type".into(), json!("image-edit"));
        let body = posted("apiframe-qwen-image", edit, &urls);
        assert_eq!(body["qwenImageParams"]["task_type"], "image-edit");
    }

    #[test]
    fn parse_upload_accepts_created_and_rejects_off_host_urls() {
        let provider = provider("apiframe-flux-2-pro");
        let ok = provider
            .parse_upload(&response(
                201,
                r#"{"url": "https://cdn2.apiframe.ai/uploads/u1.png"}"#,
            ))
            .unwrap();
        assert_eq!(
            ok,
            ImageInput::Url("https://cdn2.apiframe.ai/uploads/u1.png".into())
        );
        let plain = provider
            .parse_upload(&response(
                200,
                r#"{"url": "https://cdn2.apiframe.ai/uploads/u2.png"}"#,
            ))
            .unwrap();
        assert_eq!(
            plain,
            ImageInput::Url("https://cdn2.apiframe.ai/uploads/u2.png".into())
        );
        for (kind, body) in [
            ("off-host", r#"{"url": "https://evil.example/u1.png"}"#),
            ("missing", r#"{"uploaded": true}"#),
        ] {
            let parsed = provider.parse_upload(&response(201, body));
            assert!(
                matches!(&parsed, Err(ProviderError::Protocol(text)) if text == "unexpected upload host"),
                "{kind}: {parsed:?}"
            );
        }
        assert!(matches!(
            provider.parse_upload(&response(401, "no")),
            Err(ProviderError::Auth)
        ));
    }

    #[test]
    fn parse_submit_builds_the_poll_url_from_job_id() {
        let provider = provider("apiframe-midjourney");
        let submitted = provider
            .parse_submit(&response(
                202,
                r#"{"jobId": "a1b2c3d4-e5f6-7890-abcd-ef1234567890", "status": "QUEUED"}"#,
            ))
            .unwrap();
        assert_eq!(
            submitted.job_id.as_str(),
            "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
        );
        assert_eq!(
            submitted.poll_url.as_str(),
            "https://api.apiframe.ai/v2/jobs/a1b2c3d4-e5f6-7890-abcd-ef1234567890"
        );
        assert!(submitted.quote.is_none());
        let accepted = provider
            .parse_submit(&response(200, r#"{"jobId": "job_1"}"#))
            .unwrap();
        assert_eq!(accepted.job_id.as_str(), "job_1");
        assert!(
            provider
                .parse_submit(&response(202, r#"{"status": "QUEUED"}"#))
                .is_err()
        );
        let poll = provider.poll_request(&submitted);
        assert_eq!(poll.method, Method::Get);
        assert_eq!(poll.auth, Some(ProviderId::Apiframe));
        assert_eq!(poll.body, Body::Empty);
    }

    #[test]
    fn submit_errors_classify_by_status() {
        let provider = provider("apiframe-flux-2-pro");
        assert!(matches!(
            provider.parse_submit(&response(401, "no")).unwrap_err(),
            ProviderError::Auth
        ));
        assert!(matches!(
            provider.parse_submit(&response(403, "no")).unwrap_err(),
            ProviderError::Auth
        ));
        assert!(matches!(
            provider.parse_submit(&response(402, "no")).unwrap_err(),
            ProviderError::OutOfCredits { hint: None }
        ));
        assert!(matches!(
            provider.parse_submit(&response(404, "no")).unwrap_err(),
            ProviderError::ModelNotFound
        ));
        assert!(matches!(
            provider
                .parse_submit(&response(429, "slow down"))
                .unwrap_err(),
            ProviderError::RateLimited { retry_after: None }
        ));
        match provider
            .parse_submit(&response(
                400,
                r#"{"error": "Invalid parameters", "details": []}"#,
            ))
            .unwrap_err()
        {
            ProviderError::Validation { message, .. } => assert_eq!(message, "Invalid parameters"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            provider.parse_submit(&response(500, "boom")).unwrap_err(),
            ProviderError::Http { status: 500, .. }
        ));
    }

    #[test]
    fn poll_maps_in_flight_and_failed_statuses() {
        let provider = provider("apiframe-flux-2-pro");
        match provider.parse_poll(&response(
            200,
            r#"{"status": "PROCESSING", "progress": 65}"#,
        )) {
            PollOutcome::InFlight { status, progress } => {
                assert_eq!(status, "PROCESSING");
                assert_eq!(progress, Some(0.65));
            }
            other => panic!("{other:?}"),
        }
        match provider.parse_poll(&response(200, r#"{"status": "QUEUED", "progress": 0}"#)) {
            PollOutcome::InFlight { status, progress } => {
                assert_eq!(status, "QUEUED");
                assert_eq!(progress, Some(0.0));
            }
            other => panic!("{other:?}"),
        }
        match provider.parse_poll(&response(
            200,
            r#"{"status": "FAILED", "error": "Provider returned an error: content policy violation"}"#,
        )) {
            PollOutcome::Failed(ProviderError::Blocked) => {}
            other => panic!("{other:?}"),
        }
        match provider.parse_poll(&response(
            200,
            r#"{"status": "FAILED", "error": "worker exploded"}"#,
        )) {
            PollOutcome::Failed(ProviderError::Protocol(text)) => {
                assert_eq!(text, "worker exploded");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            provider.parse_poll(&response(429, "slow")),
            PollOutcome::Failed(ProviderError::RateLimited { retry_after: None })
        ));
    }

    #[test]
    fn a_completed_single_image_poll_carries_the_first_url() {
        let provider = provider("apiframe-seedream-5-pro");
        match provider.parse_poll(&response(
            200,
            r#"{"status": "COMPLETED", "creditCost": 5, "result": {"images": ["https://cdn2.apiframe.ai/images/a.png", "https://cdn2.apiframe.ai/images/b.png"]}}"#,
        )) {
            PollOutcome::Ready {
                url,
                settled_cost,
                grid_urls,
                ..
            } => {
                assert_eq!(url.as_str(), "https://cdn2.apiframe.ai/images/a.png");
                assert_eq!(settled_cost, Some(5.0));
                assert_eq!(grid_urls, None);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            provider.parse_poll(&response(
                200,
                r#"{"status": "COMPLETED", "result": {"images": []}}"#,
            )),
            PollOutcome::Failed(ProviderError::Protocol(_))
        ));
        assert!(matches!(
            provider.parse_poll(&response(
                200,
                r#"{"status": "COMPLETED", "result": {"images": ["https://evil.example/a.png"]}}"#,
            )),
            PollOutcome::Failed(ProviderError::Protocol(_))
        ));
    }

    #[test]
    fn a_completed_midjourney_poll_carries_the_grid_and_four_quadrants() {
        let provider = provider("apiframe-midjourney");
        let body = r#"{"status": "COMPLETED", "creditCost": 10, "result": {"images": [
            "https://cdn2.apiframe.ai/images/a-1.png",
            "https://cdn2.apiframe.ai/images/a-2.png",
            "https://cdn2.apiframe.ai/images/a-3.png",
            "https://cdn2.apiframe.ai/images/a-4.png"
        ], "gridUrl": "https://cdn2.apiframe.ai/images/a-grid.png"}}"#;
        match provider.parse_poll(&response(200, body)) {
            PollOutcome::Ready {
                url,
                settled_cost,
                grid_urls: Some(quadrants),
                ..
            } => {
                assert_eq!(url.as_str(), "https://cdn2.apiframe.ai/images/a-grid.png");
                assert_eq!(settled_cost, Some(10.0));
                assert_eq!(
                    quadrants[0].as_str(),
                    "https://cdn2.apiframe.ai/images/a-1.png"
                );
                assert_eq!(
                    quadrants[3].as_str(),
                    "https://cdn2.apiframe.ai/images/a-4.png"
                );
            }
            other => panic!("{other:?}"),
        }
        for (kind, body) in [
            (
                "three-images",
                r#"{"status": "COMPLETED", "result": {"images": ["https://cdn2.apiframe.ai/images/a-1.png", "https://cdn2.apiframe.ai/images/a-2.png", "https://cdn2.apiframe.ai/images/a-3.png"], "gridUrl": "https://cdn2.apiframe.ai/images/a-grid.png"}}"#,
            ),
            (
                "no-grid",
                r#"{"status": "COMPLETED", "result": {"images": ["https://cdn2.apiframe.ai/images/a-1.png", "https://cdn2.apiframe.ai/images/a-2.png", "https://cdn2.apiframe.ai/images/a-3.png", "https://cdn2.apiframe.ai/images/a-4.png"]}}"#,
            ),
            (
                "off-host-quadrant",
                r#"{"status": "COMPLETED", "result": {"images": ["https://cdn2.apiframe.ai/images/a-1.png", "https://evil.example/a-2.png", "https://cdn2.apiframe.ai/images/a-3.png", "https://cdn2.apiframe.ai/images/a-4.png"], "gridUrl": "https://cdn2.apiframe.ai/images/a-grid.png"}}"#,
            ),
        ] {
            let outcome = provider.parse_poll(&response(200, body));
            assert!(
                matches!(outcome, PollOutcome::Failed(ProviderError::Protocol(_))),
                "{kind}: {outcome:?}"
            );
        }
    }

    #[test]
    fn poll_policy_polls_every_two_to_three_seconds_for_ten_minutes() {
        let policy = super::poll_policy();
        assert_eq!(policy.initial, std::time::Duration::from_secs(2));
        assert_eq!(policy.next_delay(1), std::time::Duration::from_millis(2500));
        assert_eq!(policy.next_delay(2), std::time::Duration::from_millis(3000));
        assert_eq!(policy.next_delay(9), std::time::Duration::from_secs(3));
        assert_eq!(policy.deadline, std::time::Duration::from_secs(600));
    }

    #[test]
    fn prompts_must_be_between_one_and_two_thousand_chars() {
        for id in ["apiframe-midjourney", "apiframe-flux-2-pro"] {
            let errors = provider(id).validate_for_submit(&scene_with("  "), &BTreeMap::new(), 0);
            assert!(
                errors
                    .iter()
                    .any(|error| error.key == "prompt" && error.message == "Prompt is empty"),
                "{id}"
            );
            let long = "a".repeat(2001);
            let errors = provider(id).validate_for_submit(&scene_with(&long), &BTreeMap::new(), 0);
            assert!(
                errors
                    .iter()
                    .any(|error| error.key == "prompt" && error.message.contains("2000")),
                "{id}"
            );
            let errors =
                provider(id).validate_for_submit(&scene_with("a lighthouse"), &BTreeMap::new(), 0);
            assert!(!errors.iter().any(|error| error.key == "prompt"), "{id}");
        }
    }

    #[test]
    fn qwen_image_edit_requires_an_image_but_text_to_image_does_not() {
        let mut edit = defaults("apiframe-qwen-image");
        edit.insert("task_type".into(), json!("image-edit"));
        let errors =
            provider("apiframe-qwen-image").validate_for_submit(&scene_with("edit it"), &edit, 0);
        let error = errors
            .iter()
            .find(|error| error.key == "images")
            .unwrap_or_else(|| panic!("no images error: {errors:?}"));
        assert_eq!(error.message, "Add at least one image.");

        let errors = provider("apiframe-qwen-image").validate_for_submit(
            &scene_with("draw it"),
            &defaults("apiframe-qwen-image"),
            0,
        );
        assert!(!errors.iter().any(|error| error.key == "images"));

        let errors =
            provider("apiframe-qwen-image").validate_for_submit(&scene_with("edit it"), &edit, 1);
        assert!(!errors.iter().any(|error| error.key == "images"));
        for id in [
            "apiframe-midjourney",
            "apiframe-dall-e-3",
            "apiframe-flux-2-pro",
        ] {
            let errors = provider(id).validate_for_submit(&scene_with("go"), &BTreeMap::new(), 0);
            assert!(!errors.iter().any(|error| error.key == "images"), "{id}");
        }
    }

    #[test]
    fn schema_violations_surface_from_the_validator() {
        let mut values = defaults("apiframe-flux-1.1-pro");
        values.insert("width".into(), json!(100));
        let errors = super::flux_11_pro().validate_for_submit(&scene_with("go"), &values, 0);
        assert!(
            errors.iter().any(|error| error.key == "width"),
            "{errors:?}"
        );
        let mut seed = defaults("apiframe-wan-image-2.6");
        seed.insert("seed".into(), json!(-1));
        let errors = super::wan_image_2_6().validate_for_submit(&scene_with("go"), &seed, 0);
        assert!(errors.iter().any(|error| error.key == "seed"), "{errors:?}");
    }

    #[test]
    fn field_bounds_follow_the_model_table() {
        let flux = &spec_of("apiframe-flux-1.1-pro").schema;
        assert!(!flux.accepts("width", &json!(255)));
        assert!(flux.accepts("width", &json!(256)));
        assert!(flux.accepts("width", &json!(1440)));
        assert!(!flux.accepts("width", &json!(1441)));
        assert!(!flux.accepts("output_quality", &json!(101)));
        let flex = &spec_of("apiframe-flux-2-flex").schema;
        assert!(!flex.accepts("steps", &json!(0)));
        assert!(flex.accepts("steps", &json!(50)));
        assert!(!flex.accepts("guidance", &json!(1.0)));
        assert!(flex.accepts("guidance", &json!(10.0)));
        let sd = &spec_of("apiframe-stable-diffusion-3.5-large").schema;
        assert!(sd.accepts("cfg", &json!(20.0)));
        assert!(!sd.accepts("cfg", &json!(20.5)));
        let sd3 = &spec_of("apiframe-stable-diffusion-3").schema;
        assert!(sd3.accepts("steps", &json!(28)));
        assert!(!sd3.accepts("steps", &json!(29)));
        let qwen = &spec_of("apiframe-qwen-image").schema;
        assert!(!qwen.accepts("num_inference_steps", &json!(1)));
        assert!(qwen.accepts("num_inference_steps", &json!(250)));
        let wan = &spec_of("apiframe-wan-image-2.6").schema;
        assert!(wan.accepts("seed", &json!(2_147_483_647)));
        assert!(!wan.accepts("seed", &json!(-1)));
        let midjourney = &spec_of("apiframe-midjourney").schema;
        assert!(midjourney.accepts("aspect_ratio", &json!("21:9")));
        assert!(!midjourney.accepts("aspect_ratio", &json!("4:1")));
    }
}
