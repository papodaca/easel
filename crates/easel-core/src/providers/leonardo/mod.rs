//! The Leonardo.Ai provider: forty-three image models behind one async
//! `POST /api/rest/v2/generations` endpoint, polled on
//! `GET /api/rest/v1/generations/{id}` with results served from
//! `cdn.leonardo.ai`. Schemas are driven by `docs/api/leonardo.create.md`.

use std::collections::BTreeMap;
use std::sync::LazyLock;
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::error::{FieldError, ProviderError};
use crate::http::{Body, HttpRequest, HttpResponse, Method};
use crate::job::{EncodedImage, JobRequest, PollPolicy};
use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::providers::{ImageInput, JobId, PollOutcome, Provider, ReferenceStep, Submitted};
use crate::scene::Scene;
use crate::schema::{CrossRule, FieldDef, FieldKind, ParamSchema, RuleContext};

pub const GENERATIONS_URL: &str = "https://cloud.leonardo.ai/api/rest/v2/generations";
const GENERATION_URL: &str = "https://cloud.leonardo.ai/api/rest/v1/generations";
const STORED_CAP: usize = 64 * 1024;
const KREA_STYLE_MAX_IMAGES: usize = 10;

const STYLE_STRENGTHS: &[&str] = &["LOW", "MID", "HIGH", "ULTRA", "MAX"];
const CONTENT_STRENGTHS: &[&str] = &["LOW", "MID", "HIGH", "ULTRA"];
const BASIC_STRENGTHS: &[&str] = &["LOW", "MID", "HIGH"];

static SLOTS_PHOENIX: &[&str] = &["image_to_image", "content", "style", "character"];
static SLOTS_SDXL: &[&str] = &[
    "image_to_image",
    "content",
    "style",
    "character",
    "depth",
    "edge",
    "pose",
    "text_image",
];
static SLOTS_CONTENT_STYLE: &[&str] = &["content", "style"];

static MODE_FAST_QUALITY_ULTRA: &[&str] = &["FAST", "QUALITY", "ULTRA"];
static MODE_FAST_QUALITY: &[&str] = &["FAST", "QUALITY"];
static MODE_FAST_ULTRA: &[&str] = &["FAST", "ULTRA"];
static CONTRASTS: &[&str] = &["LOW", "MEDIUM", "HIGH"];
static ENHANCES: &[&str] = &["AUTO", "ON", "OFF"];
static QUALITY_THREE: &[&str] = &["TURBO", "BALANCED", "QUALITY"];
static QUALITY_FOUR: &[&str] = &["VERY_LOW", "LOW", "MEDIUM", "HIGH"];
static QUALITY_GPT: &[&str] = &["LOW", "MEDIUM", "HIGH"];
static QUALITY_GPT_MAX: &[&str] = &["LOW", "MEDIUM", "HIGH", "XHIGH", "MAX"];
static QUALITY_KREA: &[&str] = &["MEDIUM", "HIGH"];
static OUTPUT_FORMATS: &[&str] = &["jpeg", "png"];
static UPSCALE_FACTORS: &[&str] = &["2", "3", "4", "6", "8"];
static UPSCALE_MODES: &[&str] = &["clean", "detailed"];
static CREATIVITY: &[&str] = &["low", "mid", "high"];
static BG_SIZES: &[&str] = &["auto", "preview", "full", "50MP"];
static BG_TYPES: &[&str] = &[
    "auto",
    "person",
    "product",
    "car",
    "animal",
    "graphic",
    "transportation",
    "other",
];
static BG_TYPE_LEVELS: &[&str] = &["none", "1", "2", "latest"];
static BG_FORMATS: &[&str] = &["png", "jpg", "webp"];
static BG_CHANNELS: &[&str] = &["rgba", "alpha"];
static BG_SHADOW_TYPES: &[&str] = &["none", "drop", "3d", "car"];

/// Documented style labels paired with their UUIDs, then the twenty-four
/// undocumented SDXL preset UUIDs which map to themselves.
static STYLE_TABLE: &[(&str, &str)] = &[
    ("3D Render", "debdf72a-91a4-467b-bf61-cc02bdeb69c6"),
    ("Acrylic", "3cbb655a-7ca4-463f-b697-8a03ad67327c"),
    ("Anime General", "b2a54a51-230b-4d4f-ad4e-8409bf58645f"),
    ("Bokeh", "9fdc5e8c-4d13-49b4-9ce6-5a74cbb19177"),
    ("Cinematic", "a5632c7c-ddbb-4e2f-ba34-8456ab3ac436"),
    ("Cinematic Close-Up", "cc53f935-884c-40a0-b7eb-1f5c42821fb5"),
    ("Cinematic Concept", "33abbb99-03b9-4dd7-9761-ee98650b2c88"),
    ("Creative", "6fedbf1f-4a17-45ec-84fb-92fe524a29ef"),
    ("Dynamic", "111dc692-d470-4eec-b791-3475abac4c46"),
    ("Fashion", "594c4a08-a522-4e0e-b7ff-e4dac4b6b622"),
    ("Film", "85da2dcc-c373-464c-9a7a-5624359be859"),
    ("Food", "d574325d-1278-4fe2-974b-768525f253c3"),
    ("Game Concept", "09d2b5b5-d7c5-4c02-905d-9f84051640f4"),
    ("Graphic Design 2D", "703d6fe5-7f1c-4a9e-8da0-5331f214d5cf"),
    ("Graphic Design 3D", "7d7c2bc5-4b12-4ac3-81a9-630057e9e89f"),
    (
        "Graphic Design Pop Art",
        "2e74ec31-f3a4-4825-b08b-2894f6d13941",
    ),
    (
        "Graphic Design Vector",
        "1fbb6a68-9319-44d2-8d56-2957ca0ece6a",
    ),
    ("HDR", "97c20e5c-1af6-4d42-b227-54d03d8f0727"),
    ("Illustration", "645e4195-f63d-4715-a3f2-3fb1e6eb8c70"),
    ("Long Exposure", "335e6010-a75c-45d9-afc5-032c65e9180e"),
    ("Macro", "30c1d34f-e3a9-479a-b56f-c018bbc9c02a"),
    ("Minimalist", "cadc8cd6-7838-4c99-b645-df76be8ba8d8"),
    ("Monochrome", "a2f7ea66-959b-4bbe-b508-6133238b76b6"),
    ("Moody", "621e1c9a-6319-4bee-a12d-ae40659162fa"),
    ("Neutral", "0d914779-c822-430a-b976-30075033f1c4"),
    ("None", "556c1ee5-ec38-42e8-955a-1e82dad0ffa1"),
    ("Portrait", "ab5a4220-7c42-41e5-a578-eddb9fed3d75"),
    ("Portrait Cinematic", "4edb03c9-8a26-4041-9d01-f85b5d4abd71"),
    ("Portrait Fashion", "0d34f8e1-46d4-428f-8ddd-4b11811fa7c9"),
    (
        "Pro B&W Photography",
        "22a9a7d2-2166-4d86-80ff-22e2643adbcf",
    ),
    (
        "Pro Color Photography",
        "7c3f932b-a572-47cb-9b9b-f20211e63b5b",
    ),
    (
        "Pro Film Photography",
        "581ba6d6-5aac-4492-bebe-54c424a0d46e",
    ),
    ("Ray Traced", "b504f83c-3326-4947-82e1-7fe9e839ec0f"),
    ("Retro", "6105baa2-851b-446e-9db5-08a671a8c42f"),
    ("Sketch (B&W)", "be8c6b58-739c-4d44-b9c1-b032ed308b61"),
    ("Sketch (Color)", "093accc3-7633-4ffd-82da-d34000dfc0d6"),
    ("Stock Photo", "5bdc3f2a-1be6-4d1c-8e77-992a30824a2c"),
    ("Unprocessed", "62736842-6e4b-4028-b79a-4f1a1606e893"),
    ("Vibrant", "dee282d3-891f-4f73-ba02-7f8131e5541b"),
    ("Watercolor", "1db308ce-c7ad-4d10-96fd-592fa6b75cc4"),
    (
        "d574325d-1278-43fc-9159-e8ff2154e9f7",
        "d574325d-1278-43fc-9159-e8ff2154e9f7",
    ),
    (
        "b220bdb6-1378-43fc-9159-e8ff2154e9f7",
        "b220bdb6-1378-43fc-9159-e8ff2154e9f7",
    ),
    (
        "32b653d2-8819-4363-bc82-a6466dfcb825",
        "32b653d2-8819-4363-bc82-a6466dfcb825",
    ),
    (
        "5f611024-668e-41cc-b538-4569a7985cad",
        "5f611024-668e-41cc-b538-4569a7985cad",
    ),
    (
        "df2def02-1104-46c8-a857-39292fcd9091",
        "df2def02-1104-46c8-a857-39292fcd9091",
    ),
    (
        "aab2fcb9-3600-4039-9c25-a2a26515df5f",
        "aab2fcb9-3600-4039-9c25-a2a26515df5f",
    ),
    (
        "1824f984-dc0b-4f72-aa32-0442091d488c",
        "1824f984-dc0b-4f72-aa32-0442091d488c",
    ),
    (
        "16818741-bdbb-474f-a7a8-1e33d706658e",
        "16818741-bdbb-474f-a7a8-1e33d706658e",
    ),
    (
        "bf0b8b5f-735f-4141-a365-8d8aa78fd147",
        "bf0b8b5f-735f-4141-a365-8d8aa78fd147",
    ),
    (
        "5d256598-5671-424c-8152-27943f1000f1",
        "5d256598-5671-424c-8152-27943f1000f1",
    ),
    (
        "726585f5-d231-4a3a-bee9-9a5e5d3df779",
        "726585f5-d231-4a3a-bee9-9a5e5d3df779",
    ),
    (
        "ca934804-ef39-4e1b-af37-608021452444",
        "ca934804-ef39-4e1b-af37-608021452444",
    ),
    (
        "e89ca6f0-7653-4db6-8a24-ded9d1267d84",
        "e89ca6f0-7653-4db6-8a24-ded9d1267d84",
    ),
    (
        "d4fb14b9-972b-4215-b461-616a51b6eeb8",
        "d4fb14b9-972b-4215-b461-616a51b6eeb8",
    ),
    (
        "c9b08088-f497-4e30-b258-6d535d928cc5",
        "c9b08088-f497-4e30-b258-6d535d928cc5",
    ),
    (
        "0717967c-1a32-4c67-bc56-b020e97b1b81",
        "0717967c-1a32-4c67-bc56-b020e97b1b81",
    ),
    (
        "39a14739-5d21-420b-a4ef-e8a80978185a",
        "39a14739-5d21-420b-a4ef-e8a80978185a",
    ),
    (
        "3e645868-7d35-4b8c-974c-10bd8c2de972",
        "3e645868-7d35-4b8c-974c-10bd8c2de972",
    ),
    (
        "6e13a467-7dde-415a-8020-0ce2b27bc4e8",
        "6e13a467-7dde-415a-8020-0ce2b27bc4e8",
    ),
    (
        "e4d8224a-d804-4b00-b383-b1f3213b9faf",
        "e4d8224a-d804-4b00-b383-b1f3213b9faf",
    ),
    (
        "5e931eed-1ab9-48a0-aad0-ea5c7d13d472",
        "5e931eed-1ab9-48a0-aad0-ea5c7d13d472",
    ),
    (
        "fed4e15f-84b3-4f6e-9b20-9d3a3798eab8",
        "fed4e15f-84b3-4f6e-9b20-9d3a3798eab8",
    ),
    (
        "785d778b-cdd3-4cf5-8ec1-d6e995254077",
        "785d778b-cdd3-4cf5-8ec1-d6e995254077",
    ),
    (
        "ee13fe69-c387-4728-b274-de390a067200",
        "ee13fe69-c387-4728-b274-de390a067200",
    ),
];

#[must_use]
fn style_label(uuid: &str) -> &str {
    STYLE_TABLE
        .iter()
        .find(|(_, candidate)| *candidate == uuid)
        .map_or(uuid, |(label, _)| *label)
}

fn style_uuid(label: &str) -> Option<&'static str> {
    STYLE_TABLE
        .iter()
        .find(|(candidate, _)| *candidate == label)
        .map(|(_, uuid)| *uuid)
}

static STYLE_PHOENIX: &[&str] = &[
    "debdf72a-91a4-467b-bf61-cc02bdeb69c6",
    "9fdc5e8c-4d13-49b4-9ce6-5a74cbb19177",
    "a5632c7c-ddbb-4e2f-ba34-8456ab3ac436",
    "33abbb99-03b9-4dd7-9761-ee98650b2c88",
    "6fedbf1f-4a17-45ec-84fb-92fe524a29ef",
    "111dc692-d470-4eec-b791-3475abac4c46",
    "594c4a08-a522-4e0e-b7ff-e4dac4b6b622",
    "2e74ec31-f3a4-4825-b08b-2894f6d13941",
    "1fbb6a68-9319-44d2-8d56-2957ca0ece6a",
    "645e4195-f63d-4715-a3f2-3fb1e6eb8c70",
    "30c1d34f-e3a9-479a-b56f-c018bbc9c02a",
    "cadc8cd6-7838-4c99-b645-df76be8ba8d8",
    "621e1c9a-6319-4bee-a12d-ae40659162fa",
    "ab5a4220-7c42-41e5-a578-eddb9fed3d75",
    "22a9a7d2-2166-4d86-80ff-22e2643adbcf",
    "7c3f932b-a572-47cb-9b9b-f20211e63b5b",
    "581ba6d6-5aac-4492-bebe-54c424a0d46e",
    "b504f83c-3326-4947-82e1-7fe9e839ec0f",
    "be8c6b58-739c-4d44-b9c1-b032ed308b61",
    "093accc3-7633-4ffd-82da-d34000dfc0d6",
    "5bdc3f2a-1be6-4d1c-8e77-992a30824a2c",
    "dee282d3-891f-4f73-ba02-7f8131e5541b",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
];
static STYLE_LUCID: &[&str] = &[
    "a5632c7c-ddbb-4e2f-ba34-8456ab3ac436",
    "6fedbf1f-4a17-45ec-84fb-92fe524a29ef",
    "111dc692-d470-4eec-b791-3475abac4c46",
    "594c4a08-a522-4e0e-b7ff-e4dac4b6b622",
    "ab5a4220-7c42-41e5-a578-eddb9fed3d75",
    "5bdc3f2a-1be6-4d1c-8e77-992a30824a2c",
    "dee282d3-891f-4f73-ba02-7f8131e5541b",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
];
static STYLE_LUCID_REALISM: &[&str] = &[
    "9fdc5e8c-4d13-49b4-9ce6-5a74cbb19177",
    "a5632c7c-ddbb-4e2f-ba34-8456ab3ac436",
    "cc53f935-884c-40a0-b7eb-1f5c42821fb5",
    "6fedbf1f-4a17-45ec-84fb-92fe524a29ef",
    "111dc692-d470-4eec-b791-3475abac4c46",
    "594c4a08-a522-4e0e-b7ff-e4dac4b6b622",
    "85da2dcc-c373-464c-9a7a-5624359be859",
    "d574325d-1278-4fe2-974b-768525f253c3",
    "97c20e5c-1af6-4d42-b227-54d03d8f0727",
    "335e6010-a75c-45d9-afc5-032c65e9180e",
    "30c1d34f-e3a9-479a-b56f-c018bbc9c02a",
    "cadc8cd6-7838-4c99-b645-df76be8ba8d8",
    "a2f7ea66-959b-4bbe-b508-6133238b76b6",
    "621e1c9a-6319-4bee-a12d-ae40659162fa",
    "0d914779-c822-430a-b976-30075033f1c4",
    "ab5a4220-7c42-41e5-a578-eddb9fed3d75",
    "6105baa2-851b-446e-9db5-08a671a8c42f",
    "5bdc3f2a-1be6-4d1c-8e77-992a30824a2c",
    "62736842-6e4b-4028-b79a-4f1a1606e893",
    "dee282d3-891f-4f73-ba02-7f8131e5541b",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
];
static STYLE_KINO: &[&str] = &[
    "a5632c7c-ddbb-4e2f-ba34-8456ab3ac436",
    "9fdc5e8c-4d13-49b4-9ce6-5a74cbb19177",
    "cc53f935-884c-40a0-b7eb-1f5c42821fb5",
    "6fedbf1f-4a17-45ec-84fb-92fe524a29ef",
    "594c4a08-a522-4e0e-b7ff-e4dac4b6b622",
    "85da2dcc-c373-464c-9a7a-5624359be859",
    "d574325d-1278-43fc-9159-e8ff2154e9f7",
    "97c20e5c-1af6-4d42-b227-54d03d8f0727",
    "b220bdb6-1378-43fc-9159-e8ff2154e9f7",
    "30c1d34f-e3a9-479a-b56f-c018bbc9c02a",
    "cadc8cd6-7838-4c99-b645-df76be8ba8d8",
    "a2f7ea66-959b-4bbe-b508-6133238b76b6",
    "621e1c9a-6319-4bee-a12d-ae40659162fa",
    "0d914779-c822-430a-b976-30075033f1c4",
    "6105baa2-851b-446e-9db5-08a671a8c42f",
    "5bdc3f2a-1be6-4d1c-8e77-992a30824a2c",
    "62736842-6e4b-4028-b79a-4f1a1606e893",
    "dee282d3-891f-4f73-ba02-7f8131e5541b",
    "111dc692-d470-4eec-b791-3475abac4c46",
    "ab5a4220-7c42-41e5-a578-eddb9fed3d75",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
];
static STYLE_ANIME: &[&str] = &[
    "32b653d2-8819-4363-bc82-a6466dfcb825",
    "5f611024-668e-41cc-b538-4569a7985cad",
    "df2def02-1104-46c8-a857-39292fcd9091",
    "aab2fcb9-3600-4039-9c25-a2a26515df5f",
    "1824f984-dc0b-4f72-aa32-0442091d488c",
    "16818741-bdbb-474f-a7a8-1e33d706658e",
    "bf0b8b5f-735f-4141-a365-8d8aa78fd147",
    "5d256598-5671-424c-8152-27943f1000f1",
    "726585f5-d231-4a3a-bee9-9a5e5d3df779",
    "b2a54a51-230b-4d4f-ad4e-8409bf58645f",
    "ca934804-ef39-4e1b-af37-608021452444",
    "e89ca6f0-7653-4db6-8a24-ded9d1267d84",
];
static STYLE_PORTRAIT: &[&str] = &[
    "4edb03c9-8a26-4041-9d01-f85b5d4abd71",
    "d4fb14b9-972b-4215-b461-616a51b6eeb8",
    "c9b08088-f497-4e30-b258-6d535d928cc5",
    "0717967c-1a32-4c67-bc56-b020e97b1b81",
    "39a14739-5d21-420b-a4ef-e8a80978185a",
    "3e645868-7d35-4b8c-974c-10bd8c2de972",
    "0d34f8e1-46d4-428f-8ddd-4b11811fa7c9",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
];
static STYLE_STOCK: &[&str] = &[
    "5bdc3f2a-1be6-4d1c-8e77-992a30824a2c",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
];
static STYLE_ALBEDO: &[&str] = &[
    "645e4195-f63d-4715-a3f2-3fb1e6eb8c70",
    "debdf72a-91a4-467b-bf61-cc02bdeb69c6",
    "b504f83c-3326-4947-82e1-7fe9e839ec0f",
    "6e13a467-7dde-415a-8020-0ce2b27bc4e8",
    "621e1c9a-6319-4bee-a12d-ae40659162fa",
    "ab5a4220-7c42-41e5-a578-eddb9fed3d75",
    "b2a54a51-230b-4d4f-ad4e-8409bf58645f",
    "e4d8224a-d804-4b00-b383-b1f3213b9faf",
    "5e931eed-1ab9-48a0-aad0-ea5c7d13d472",
    "cadc8cd6-7838-4c99-b645-df76be8ba8d8",
    "d574325d-1278-4fe2-974b-768525f253c3",
    "fed4e15f-84b3-4f6e-9b20-9d3a3798eab8",
    "be8c6b58-739c-4d44-b9c1-b032ed308b61",
    "093accc3-7633-4ffd-82da-d34000dfc0d6",
    "111dc692-d470-4eec-b791-3475abac4c46",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
];
static STYLE_CONCEPT: &[&str] = &[
    "726585f5-d231-4a3a-bee9-9a5e5d3df779",
    "5d256598-5671-424c-8152-27943f1000f1",
    "09d2b5b5-d7c5-4c02-905d-9f84051640f4",
    "6e13a467-7dde-415a-8020-0ce2b27bc4e8",
    "33abbb99-03b9-4dd7-9761-ee98650b2c88",
    "fed4e15f-84b3-4f6e-9b20-9d3a3798eab8",
    "5e931eed-1ab9-48a0-aad0-ea5c7d13d472",
    "e4d8224a-d804-4b00-b383-b1f3213b9faf",
];
static STYLE_LIFELIKE: &[&str] = &[
    "6fedbf1f-4a17-45ec-84fb-92fe524a29ef",
    "debdf72a-91a4-467b-bf61-cc02bdeb69c6",
    "a5632c7c-ddbb-4e2f-ba34-8456ab3ac436",
    "ab5a4220-7c42-41e5-a578-eddb9fed3d75",
    "645e4195-f63d-4715-a3f2-3fb1e6eb8c70",
    "594c4a08-a522-4e0e-b7ff-e4dac4b6b622",
    "dee282d3-891f-4f73-ba02-7f8131e5541b",
    "b504f83c-3326-4947-82e1-7fe9e839ec0f",
    "6105baa2-851b-446e-9db5-08a671a8c42f",
    "621e1c9a-6319-4bee-a12d-ae40659162fa",
    "a2f7ea66-959b-4bbe-b508-6133238b76b6",
    "cadc8cd6-7838-4c99-b645-df76be8ba8d8",
    "30c1d34f-e3a9-479a-b56f-c018bbc9c02a",
    "b220bdb6-1378-43fc-9159-e8ff2154e9f7",
    "85da2dcc-c373-464c-9a7a-5624359be859",
    "97c20e5c-1af6-4d42-b227-54d03d8f0727",
    "cc53f935-884c-40a0-b7eb-1f5c42821fb5",
    "9fdc5e8c-4d13-49b4-9ce6-5a74cbb19177",
    "111dc692-d470-4eec-b791-3475abac4c46",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
];
static STYLE_GRAPHIC: &[&str] = &[
    "703d6fe5-7f1c-4a9e-8da0-5331f214d5cf",
    "7d7c2bc5-4b12-4ac3-81a9-630057e9e89f",
    "1fbb6a68-9319-44d2-8d56-2957ca0ece6a",
    "2e74ec31-f3a4-4825-b08b-2894f6d13941",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
    "785d778b-cdd3-4cf5-8ec1-d6e995254077",
    "ee13fe69-c387-4728-b274-de390a067200",
];
static STYLE_LIGHTNING: &[&str] = &[
    "a5632c7c-ddbb-4e2f-ba34-8456ab3ac436",
    "dee282d3-891f-4f73-ba02-7f8131e5541b",
    "645e4195-f63d-4715-a3f2-3fb1e6eb8c70",
    "6fedbf1f-4a17-45ec-84fb-92fe524a29ef",
    "594c4a08-a522-4e0e-b7ff-e4dac4b6b622",
    "5bdc3f2a-1be6-4d1c-8e77-992a30824a2c",
    "ab5a4220-7c42-41e5-a578-eddb9fed3d75",
    "debdf72a-91a4-467b-bf61-cc02bdeb69c6",
    "b504f83c-3326-4947-82e1-7fe9e839ec0f",
    "62736842-6e4b-4028-b79a-4f1a1606e893",
    "6105baa2-851b-446e-9db5-08a671a8c42f",
    "0d914779-c822-430a-b976-30075033f1c4",
    "a2f7ea66-959b-4bbe-b508-6133238b76b6",
    "621e1c9a-6319-4bee-a12d-ae40659162fa",
    "cadc8cd6-7838-4c99-b645-df76be8ba8d8",
    "d574325d-1278-4fe2-974b-768525f253c3",
    "85da2dcc-c373-464c-9a7a-5624359be859",
    "9fdc5e8c-4d13-49b4-9ce6-5a74cbb19177",
    "be8c6b58-739c-4d44-b9c1-b032ed308b61",
    "093accc3-7633-4ffd-82da-d34000dfc0d6",
    "111dc692-d470-4eec-b791-3475abac4c46",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
];
static STYLE_FLUX: &[&str] = &[
    "debdf72a-91a4-467b-bf61-cc02bdeb69c6",
    "3cbb655a-7ca4-463f-b697-8a03ad67327c",
    "b2a54a51-230b-4d4f-ad4e-8409bf58645f",
    "6fedbf1f-4a17-45ec-84fb-92fe524a29ef",
    "111dc692-d470-4eec-b791-3475abac4c46",
    "594c4a08-a522-4e0e-b7ff-e4dac4b6b622",
    "09d2b5b5-d7c5-4c02-905d-9f84051640f4",
    "7d7c2bc5-4b12-4ac3-81a9-630057e9e89f",
    "645e4195-f63d-4715-a3f2-3fb1e6eb8c70",
    "ab5a4220-7c42-41e5-a578-eddb9fed3d75",
    "4edb03c9-8a26-4041-9d01-f85b5d4abd71",
    "0d34f8e1-46d4-428f-8ddd-4b11811fa7c9",
    "b504f83c-3326-4947-82e1-7fe9e839ec0f",
    "5bdc3f2a-1be6-4d1c-8e77-992a30824a2c",
    "1db308ce-c7ad-4d10-96fd-592fa6b75cc4",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
];
static STYLE_KONTEXT: &[&str] = &[
    "debdf72a-91a4-467b-bf61-cc02bdeb69c6",
    "3cbb655a-7ca4-463f-b697-8a03ad67327c",
    "6fedbf1f-4a17-45ec-84fb-92fe524a29ef",
    "111dc692-d470-4eec-b791-3475abac4c46",
    "594c4a08-a522-4e0e-b7ff-e4dac4b6b622",
    "09d2b5b5-d7c5-4c02-905d-9f84051640f4",
    "703d6fe5-7f1c-4a9e-8da0-5331f214d5cf",
    "7d7c2bc5-4b12-4ac3-81a9-630057e9e89f",
    "645e4195-f63d-4715-a3f2-3fb1e6eb8c70",
    "ab5a4220-7c42-41e5-a578-eddb9fed3d75",
    "4edb03c9-8a26-4041-9d01-f85b5d4abd71",
    "0d34f8e1-46d4-428f-8ddd-4b11811fa7c9",
    "22a9a7d2-2166-4d86-80ff-22e2643adbcf",
    "7c3f932b-a572-47cb-9b9b-f20211e63b5b",
    "581ba6d6-5aac-4492-bebe-54c424a0d46e",
    "b504f83c-3326-4947-82e1-7fe9e839ec0f",
    "5bdc3f2a-1be6-4d1c-8e77-992a30824a2c",
    "1db308ce-c7ad-4d10-96fd-592fa6b75cc4",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
];
static STYLE_SEEDREAM: &[&str] = &[
    "a5632c7c-ddbb-4e2f-ba34-8456ab3ac436",
    "6fedbf1f-4a17-45ec-84fb-92fe524a29ef",
    "111dc692-d470-4eec-b791-3475abac4c46",
    "594c4a08-a522-4e0e-b7ff-e4dac4b6b622",
    "ab5a4220-7c42-41e5-a578-eddb9fed3d75",
    "5bdc3f2a-1be6-4d1c-8e77-992a30824a2c",
    "dee282d3-891f-4f73-ba02-7f8131e5541b",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
];
static STYLE_GPT: &[&str] = &[
    "debdf72a-91a4-467b-bf61-cc02bdeb69c6",
    "3cbb655a-7ca4-463f-b697-8a03ad67327c",
    "a5632c7c-ddbb-4e2f-ba34-8456ab3ac436",
    "6fedbf1f-4a17-45ec-84fb-92fe524a29ef",
    "111dc692-d470-4eec-b791-3475abac4c46",
    "594c4a08-a522-4e0e-b7ff-e4dac4b6b622",
    "09d2b5b5-d7c5-4c02-905d-9f84051640f4",
    "703d6fe5-7f1c-4a9e-8da0-5331f214d5cf",
    "7d7c2bc5-4b12-4ac3-81a9-630057e9e89f",
    "645e4195-f63d-4715-a3f2-3fb1e6eb8c70",
    "556c1ee5-ec38-42e8-955a-1e82dad0ffa1",
    "ab5a4220-7c42-41e5-a578-eddb9fed3d75",
    "4edb03c9-8a26-4041-9d01-f85b5d4abd71",
    "0d34f8e1-46d4-428f-8ddd-4b11811fa7c9",
    "22a9a7d2-2166-4d86-80ff-22e2643adbcf",
    "7c3f932b-a572-47cb-9b9b-f20211e63b5b",
    "581ba6d6-5aac-4492-bebe-54c424a0d46e",
    "b504f83c-3326-4947-82e1-7fe9e839ec0f",
    "5bdc3f2a-1be6-4d1c-8e77-992a30824a2c",
    "dee282d3-891f-4f73-ba02-7f8131e5541b",
    "1db308ce-c7ad-4d10-96fd-592fa6b75cc4",
];

static FLUX3_DIMS: &[i64] = &[
    672, 720, 768, 832, 864, 880, 912, 1024, 1152, 1184, 1216, 1248, 1344, 1360, 1456, 1536, 1568,
    1680, 1728, 1776, 1824, 2048, 2288, 2368, 2416, 2512, 2688, 2736, 2896, 3072, 3136, 3344, 3456,
    3552, 3664, 4096, 4576, 4736, 4848, 5024, 5456, 5792, 6256,
];
static IDEOGRAM_V4_DIMS: &[i64] = &[
    1152, 1248, 1280, 1296, 1440, 1600, 1664, 1728, 1792, 2048, 2240, 2304, 2496, 2560, 2880, 2944,
    3072, 3168, 3328,
];
static P_IMAGE_DIMS: &[i64] = &[
    512, 640, 720, 800, 832, 864, 896, 1024, 1120, 1152, 1248, 1280, 1440, 1536, 1600, 1664, 1728,
    1792, 2048, 2240, 2304, 2496, 2560, 2880, 3072, 3456,
];
static IDEOGRAM_45_DIMS: &[i64] = &[
    720, 800, 832, 864, 896, 1024, 1120, 1152, 1248, 1280, 1296, 1440, 1600, 1664, 1728, 1792,
    2048, 2240, 2304, 2496, 2560, 2880, 2944, 3072, 3168, 3328,
];
static GPT_IMAGE_W_DIMS: &[i64] = &[
    768, 832, 848, 864, 896, 928, 1024, 1088, 1136, 1152, 1184, 1200, 1248, 1264, 1344, 1376, 1536,
    1584, 1648, 1696, 1792, 1856, 2016, 2048, 2336, 2448, 2560, 2880, 3200, 3264, 3504, 3584, 3808,
];
static GPT_IMAGE_H_DIMS: &[i64] = &[
    640, 672, 768, 832, 848, 864, 896, 928, 1024, 1136, 1152, 1184, 1200, 1248, 1264, 1280, 1344,
    1376, 1536, 1584, 1632, 1648, 1696, 1792, 1824, 1856, 1920, 2016, 2048, 2336, 2448, 2560, 2880,
    3200, 3264, 3504, 3584,
];
static RECRAFT_DIMS: &[i64] = &[768, 832, 896, 1024, 1152, 1216, 1280, 1344, 1536];
static RECRAFT_PRO_DIMS: &[i64] = &[1536, 1664, 1792, 2048, 2304, 2432, 2560, 2688, 3072];
static NANO_LITE_W_DIMS: &[i64] = &[0, 768, 848, 896, 928, 1024, 1152, 1200, 1264, 1376, 1584];
static NANO_LITE_H_DIMS: &[i64] = &[0, 672, 768, 848, 896, 928, 1024, 1152, 1200, 1264, 1376];
static GEMINI25_W_DIMS: &[i64] = &[0, 768, 832, 864, 896, 1024, 1152, 1184, 1248, 1344, 1536];
static GEMINI25_H_DIMS: &[i64] = &[0, 672, 768, 832, 864, 896, 1024, 1152, 1184, 1248, 1344];
static NANO2_W_DIMS: &[i64] = &[
    0, 768, 848, 896, 928, 1024, 1152, 1200, 1264, 1376, 1536, 1584, 1696, 1792, 1856, 2048, 2304,
    2400, 2528, 2752, 3072, 3168, 3392, 3584, 3712, 4096, 4608, 4800, 5056, 5504, 6336,
];
static NANO2_H_DIMS: &[i64] = &[
    0, 672, 768, 848, 896, 928, 1024, 1152, 1200, 1264, 1344, 1376, 1536, 1696, 1792, 1856, 2048,
    2304, 2400, 2528, 2688, 2752, 3072, 3392, 3584, 3712, 4096, 4608, 4800, 5056, 5504,
];
static GEMINI_25_PAIRS: &[(i64, i64)] = &[
    (1024, 1024),
    (832, 1248),
    (1248, 832),
    (864, 1184),
    (1184, 864),
    (896, 1152),
    (1152, 896),
    (1344, 768),
    (768, 1344),
    (1536, 672),
    (0, 0),
];
static NANO2_PAIRS: &[(i64, i64)] = &[
    (768, 768),
    (848, 848),
    (896, 896),
    (928, 928),
    (1024, 1024),
    (1152, 1152),
    (1200, 1200),
    (1264, 1264),
    (1376, 1376),
    (1536, 1536),
    (1696, 1696),
    (1792, 1792),
    (1856, 1856),
    (2048, 2048),
    (2304, 2304),
    (2400, 2400),
    (2528, 2528),
    (2752, 2752),
    (3072, 3072),
    (3392, 3392),
    (3584, 3584),
    (3712, 3712),
    (4096, 4096),
    (4608, 4608),
    (4800, 4800),
    (5056, 5056),
    (5504, 5504),
    (848, 1264),
    (1696, 2528),
    (3392, 5056),
    (1264, 848),
    (2528, 1696),
    (5056, 3392),
    (896, 1200),
    (1792, 2400),
    (3584, 4800),
    (1200, 896),
    (2400, 1792),
    (4800, 3584),
    (928, 1152),
    (1856, 2304),
    (3712, 4608),
    (1152, 928),
    (2304, 1856),
    (4608, 3712),
    (768, 1376),
    (1536, 2752),
    (3072, 5504),
    (1376, 768),
    (2752, 1536),
    (5504, 3072),
    (1584, 672),
    (3168, 1344),
    (6336, 2688),
    (0, 0),
];
static NANO21_PAIRS: &[(i64, i64)] = &[
    (768, 768),
    (848, 848),
    (896, 896),
    (928, 928),
    (1024, 1024),
    (1152, 1152),
    (1200, 1200),
    (1264, 1264),
    (1376, 1376),
    (1696, 1696),
    (1792, 1792),
    (1856, 1856),
    (2048, 2048),
    (2304, 2304),
    (2400, 2400),
    (2528, 2528),
    (2752, 2752),
    (3392, 3392),
    (3584, 3584),
    (3712, 3712),
    (4096, 4096),
    (4608, 4608),
    (4800, 4800),
    (5056, 5056),
    (5504, 5504),
    (848, 1264),
    (1696, 2528),
    (3392, 5056),
    (1264, 848),
    (2528, 1696),
    (5056, 3392),
    (896, 1200),
    (1792, 2400),
    (3584, 4800),
    (1200, 896),
    (2400, 1792),
    (4800, 3584),
    (928, 1152),
    (1856, 2304),
    (3712, 4608),
    (1152, 928),
    (2304, 1856),
    (4608, 3712),
    (768, 1376),
    (1536, 2752),
    (3072, 5504),
    (1376, 768),
    (2752, 1536),
    (5504, 3072),
    (1584, 672),
    (3168, 1344),
    (6336, 2688),
    (0, 0),
];
static IDEOGRAM_V4_PAIRS: &[(i64, i64)] = &[
    (2048, 2048),
    (1440, 2880),
    (2880, 1440),
    (1664, 2496),
    (2496, 1664),
    (1792, 2240),
    (2240, 1792),
    (1440, 2560),
    (2560, 1440),
    (1600, 2560),
    (2560, 1600),
    (1728, 2304),
    (2304, 1728),
    (1296, 3168),
    (3168, 1296),
    (1152, 2944),
    (2944, 1152),
    (1248, 3328),
    (3328, 1248),
    (1280, 3072),
    (3072, 1280),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptRule {
    Required { max_chars: usize },
    Unused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionRule {
    Range {
        min_w: i64,
        max_w: i64,
        min_h: i64,
        max_h: i64,
        def_w: i64,
        def_h: i64,
        multiple_of_8: bool,
    },
    Discrete {
        w: &'static [i64],
        h: &'static [i64],
        pairs: &'static [(i64, i64)],
        def_w: i64,
        def_h: i64,
    },
    FromInputImage {
        min_side: u32,
    },
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuidanceMode {
    None,
    MultiSlot {
        slots: &'static [&'static str],
        max_images: usize,
    },
    KreaStyle,
    ImageReference {
        max_images: usize,
    },
    RemoveBg,
}

/// A model-specific schema field. `Style`, `GuidanceType`, and
/// `GuidanceStrength` drive the form but are never serialized directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extra {
    Select {
        key: &'static str,
        options: &'static [&'static str],
        default: Option<&'static str>,
    },
    Text {
        key: &'static str,
    },
    Bool {
        key: &'static str,
        default: Option<bool>,
    },
    Seed {
        max: i64,
    },
    Int {
        key: &'static str,
        min: i64,
        max: i64,
        default: Option<i64>,
    },
    UpscaleFactor,
    Style,
    GuidanceType {
        options: &'static [&'static str],
        default: &'static str,
    },
    GuidanceStrength,
}

impl Extra {
    fn key(self) -> &'static str {
        match self {
            Self::Select { key, .. }
            | Self::Text { key }
            | Self::Bool { key, .. }
            | Self::Int { key, .. } => key,
            Self::Seed { .. } => "seed",
            Self::UpscaleFactor => "upscale_factor",
            Self::Style => "style",
            Self::GuidanceType { .. } => "guidance_type",
            Self::GuidanceStrength => "guidance_strength",
        }
    }

    fn ui_only(self) -> bool {
        matches!(self, Self::GuidanceType { .. } | Self::GuidanceStrength)
    }

    fn field_def(self, desc: &ModelDesc) -> FieldDef {
        match self {
            Self::Select {
                key,
                options,
                default,
            } => select_field(key, options, default),
            Self::Text { key } => text_field(key),
            Self::Bool { key, default } => bool_field(key, default),
            Self::Seed { max } => FieldDef {
                key: "seed".into(),
                label: "Seed".into(),
                help: None,
                kind: FieldKind::Int {
                    min: 0,
                    max,
                    step: 1,
                },
                required: false,
                default: None,
                locked_when: None,
            },
            Self::Int {
                key,
                min,
                max,
                default,
            } => FieldDef {
                key: key.into(),
                label: select_label(key),
                help: None,
                kind: FieldKind::Int { min, max, step: 1 },
                required: false,
                default: default.map(|value| json!(value)),
                locked_when: None,
            },
            Self::UpscaleFactor => select_field("upscale_factor", UPSCALE_FACTORS, Some("2")),
            Self::Style => {
                let labels: Vec<&'static str> =
                    desc.styles.iter().map(|uuid| style_label(uuid)).collect();
                select_field("style", &labels, Some(desc.default_style))
            }
            Self::GuidanceType { options, default } => {
                select_field("guidance_type", options, Some(default))
            }
            Self::GuidanceStrength => {
                select_field("guidance_strength", STYLE_STRENGTHS, Some("MID"))
            }
        }
    }
}

const fn sel(key: &'static str, options: &'static [&'static str], default: &'static str) -> Extra {
    Extra::Select {
        key,
        options,
        default: Some(default),
    }
}

const fn opt_sel(key: &'static str, options: &'static [&'static str]) -> Extra {
    Extra::Select {
        key,
        options,
        default: None,
    }
}

const fn text(key: &'static str) -> Extra {
    Extra::Text { key }
}

const fn boolean(key: &'static str, default: Option<bool>) -> Extra {
    Extra::Bool { key, default }
}

const fn seed(max: i64) -> Extra {
    Extra::Seed { max }
}

const ENHANCE_AUTO: Extra = sel("prompt_enhance", ENHANCES, "AUTO");
const ENHANCE_OFF: Extra = sel("prompt_enhance", ENHANCES, "OFF");
const CONTRAST_MEDIUM: Extra = sel("contrast", CONTRASTS, "MEDIUM");
const CONTRAST_LOW: Extra = sel("contrast", CONTRASTS, "LOW");
const NEGATIVE_PROMPT: Extra = text("negative_prompt");
const TILING: Extra = boolean("tiling", None);
const TRANSPARENCY_FALSE: Extra = boolean("transparency", Some(false));
const TRANSPARENCY: Extra = boolean("transparency", None);
const STYLE: Extra = Extra::Style;
const GUIDANCE_STRENGTH: Extra = Extra::GuidanceStrength;

const fn guidance_type(options: &'static [&'static str], default: &'static str) -> Extra {
    Extra::GuidanceType { options, default }
}

fn guidance_strength_of(params: &BTreeMap<String, Value>) -> &str {
    params
        .get("guidance_strength")
        .and_then(Value::as_str)
        .unwrap_or("MID")
}

fn guidance_type_of<'a>(desc: &ModelDesc, params: &'a BTreeMap<String, Value>) -> &'a str {
    params
        .get("guidance_type")
        .and_then(Value::as_str)
        .unwrap_or(guidance_type_default(desc))
}

fn guidance_type_default(desc: &ModelDesc) -> &'static str {
    desc.fields
        .iter()
        .find_map(|extra| match extra {
            Extra::GuidanceType { default, .. } => Some(*default),
            _ => None,
        })
        .unwrap_or("image_to_image")
}

#[derive(Debug)]
pub struct ModelDesc {
    pub id: &'static str,
    pub name: &'static str,
    pub wire_model: &'static str,
    pub prompt: PromptRule,
    pub dims: DimensionRule,
    pub guidance: GuidanceMode,
    pub styles: &'static [&'static str],
    pub default_style: &'static str,
    pub fields: &'static [Extra],
}

impl ModelDesc {
    fn schema_fields(&self) -> Vec<FieldDef> {
        let mut fields = Vec::new();
        match &self.dims {
            DimensionRule::Range {
                min_w,
                max_w,
                min_h,
                max_h,
                def_w,
                def_h,
                ..
            } => {
                fields.push(int_field("width", "Width", *min_w, *max_w, Some(*def_w)));
                fields.push(int_field("height", "Height", *min_h, *max_h, Some(*def_h)));
            }
            DimensionRule::Discrete {
                w, h, def_w, def_h, ..
            } => {
                fields.push(int_field(
                    "width",
                    "Width",
                    w.first().copied().unwrap_or(0),
                    w.last().copied().unwrap_or(0),
                    Some(*def_w),
                ));
                fields.push(int_field(
                    "height",
                    "Height",
                    h.first().copied().unwrap_or(0),
                    h.last().copied().unwrap_or(0),
                    Some(*def_h),
                ));
            }
            DimensionRule::FromInputImage { .. } | DimensionRule::None => {}
        }
        for extra in self.fields {
            fields.push(extra.field_def(self));
        }
        fields
    }

    fn cross_rules(&self) -> Vec<CrossRule> {
        match self.dims {
            DimensionRule::Range {
                multiple_of_8: true,
                ..
            } => {
                vec![CrossRule::MultipleOf {
                    keys: vec!["width".into(), "height".into()],
                    n: 8,
                    exempt_zero: true,
                }]
            }
            _ => Vec::new(),
        }
    }

    fn image_slots(&self) -> ImageSlots {
        match self.guidance {
            GuidanceMode::None => ImageSlots::None,
            GuidanceMode::MultiSlot { max_images, .. } => ImageSlots::List {
                max: max_images,
                key: "guidances".into(),
            },
            GuidanceMode::KreaStyle => ImageSlots::List {
                max: KREA_STYLE_MAX_IMAGES,
                key: "style".into(),
            },
            GuidanceMode::ImageReference { max_images } => ImageSlots::List {
                max: max_images,
                key: "image_reference".into(),
            },
            GuidanceMode::RemoveBg => ImageSlots::List {
                max: 2,
                key: "image_reference".into(),
            },
        }
    }

    fn build_spec(&self) -> ModelSpec {
        ModelSpec {
            id: self.id.into(),
            display_name: self.name.into(),
            provider: ProviderId::Leonardo,
            auth: AuthScheme::Bearer,
            endpoint: GENERATIONS_URL.into(),
            beta: false,
            capabilities: Capabilities {
                supports_boxes: false,
                image_slots: self.image_slots(),
                size_control: SizeControl::WidthHeight,
            },
            schema: ParamSchema {
                fields: self.schema_fields(),
                cross_rules: self.cross_rules(),
            },
        }
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

fn select_field(key: &str, options: &[&str], default: Option<&str>) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: select_label(key),
        help: None,
        kind: FieldKind::Select {
            options: options.iter().map(|option| (*option).to_owned()).collect(),
        },
        required: false,
        default: default.map(|value| json!(value)),
        locked_when: None,
    }
}

fn select_label(key: &str) -> String {
    match key {
        "prompt_enhance" => "Prompt enhance".into(),
        "upscale_factor" => "Upscale factor".into(),
        "guidance_type" => "Guidance type".into(),
        "guidance_strength" => "Guidance strength".into(),
        "output_format" => "Output format".into(),
        "type_level" => "Type level".into(),
        "shadow_type" => "Shadow type".into(),
        "shadow_opacity" => "Shadow opacity".into(),
        "crop_margin" => "Crop margin".into(),
        "bg_color" => "Background color".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        }
    }
}

fn text_field(key: &str) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: select_label(key),
        help: None,
        kind: FieldKind::Text { multiline: false },
        required: false,
        default: None,
        locked_when: None,
    }
}

fn bool_field(key: &str, default: Option<bool>) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: select_label(key),
        help: None,
        kind: FieldKind::Bool,
        required: false,
        default: default.map(|value| json!(value)),
        locked_when: None,
    }
}

/// A descriptor-backed Leonardo.Ai model.
#[derive(Debug, Clone, Copy)]
pub struct LeonardoModel(&'static ModelDesc);

static MODELS: [ModelDesc; 43] = [
    ModelDesc {
        id: "leonardo-phoenix-v1.0",
        name: "Phoenix 1.0",
        wire_model: "phoenix-v1.0",
        prompt: PromptRule::Required { max_chars: 2000 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 2048,
            min_h: 32,
            max_h: 2048,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_PHOENIX,
            max_images: 4,
        },
        styles: STYLE_PHOENIX,
        default_style: "Dynamic",
        fields: &[
            sel("mode", MODE_FAST_QUALITY_ULTRA, "FAST"),
            CONTRAST_MEDIUM,
            ENHANCE_AUTO,
            STYLE,
            NEGATIVE_PROMPT,
            TILING,
            seed(4_294_967_295),
            guidance_type(SLOTS_PHOENIX, "image_to_image"),
            GUIDANCE_STRENGTH,
        ],
    },
    ModelDesc {
        id: "leonardo-phoenix-v0.9",
        name: "Phoenix 0.9",
        wire_model: "phoenix-v0.9",
        prompt: PromptRule::Required { max_chars: 2000 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 2048,
            min_h: 32,
            max_h: 2048,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_PHOENIX,
            max_images: 4,
        },
        styles: STYLE_PHOENIX,
        default_style: "Dynamic",
        fields: &[
            sel("mode", MODE_FAST_QUALITY_ULTRA, "FAST"),
            CONTRAST_MEDIUM,
            ENHANCE_AUTO,
            STYLE,
            NEGATIVE_PROMPT,
            TILING,
            seed(4_294_967_295),
            guidance_type(SLOTS_PHOENIX, "image_to_image"),
            GUIDANCE_STRENGTH,
        ],
    },
    ModelDesc {
        id: "leonardo-lucid-origin",
        name: "Lucid Origin",
        wire_model: "lucid-origin",
        prompt: PromptRule::Required { max_chars: 2000 },
        dims: DimensionRule::Range {
            min_w: 16,
            max_w: 3840,
            min_h: 16,
            max_h: 3616,
            def_w: 1200,
            def_h: 1200,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_CONTENT_STYLE,
            max_images: 1,
        },
        styles: STYLE_LUCID,
        default_style: "Dynamic",
        fields: &[
            sel("mode", MODE_FAST_ULTRA, "FAST"),
            ENHANCE_AUTO,
            STYLE,
            seed(2_147_483_647),
            guidance_type(SLOTS_CONTENT_STYLE, "content"),
            GUIDANCE_STRENGTH,
        ],
    },
    ModelDesc {
        id: "leonardo-lucid-realism",
        name: "Lucid Realism",
        wire_model: "lucid-realism",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 16,
            max_w: 2496,
            min_h: 16,
            max_h: 2496,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_CONTENT_STYLE,
            max_images: 1,
        },
        styles: STYLE_LUCID_REALISM,
        default_style: "Dynamic",
        fields: &[
            sel("mode", MODE_FAST_ULTRA, "FAST"),
            ENHANCE_AUTO,
            STYLE,
            seed(2_147_483_647),
            guidance_type(SLOTS_CONTENT_STYLE, "content"),
            GUIDANCE_STRENGTH,
        ],
    },
    ModelDesc {
        id: "leonardo-kino-xl",
        name: "Cinematic Kino",
        wire_model: "kino-xl",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 1536,
            min_h: 32,
            max_h: 1536,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_SDXL,
            max_images: 4,
        },
        styles: STYLE_KINO,
        default_style: "Cinematic",
        fields: SDXL_FIELDS_FAST_MEDIUM,
    },
    ModelDesc {
        id: "leonardo-anime-xl",
        name: "Anime",
        wire_model: "anime-xl",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 1584,
            min_h: 32,
            max_h: 1536,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_SDXL,
            max_images: 4,
        },
        styles: STYLE_ANIME,
        default_style: "Anime General",
        fields: SDXL_FIELDS_FAST_LOW,
    },
    ModelDesc {
        id: "leonardo-portrait-perfect",
        name: "Portrait Perfect",
        wire_model: "portrait-perfect",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 1536,
            min_h: 32,
            max_h: 1536,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_SDXL,
            max_images: 4,
        },
        styles: STYLE_PORTRAIT,
        default_style: "Portrait Cinematic",
        fields: SDXL_FIELDS_QUALITY_MEDIUM,
    },
    ModelDesc {
        id: "leonardo-stock-photography",
        name: "Stock Photography",
        wire_model: "stock-photography",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 1536,
            min_h: 32,
            max_h: 1536,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_SDXL,
            max_images: 4,
        },
        styles: STYLE_STOCK,
        default_style: "Stock Photo",
        fields: SDXL_FIELDS_QUALITY_MEDIUM,
    },
    ModelDesc {
        id: "leonardo-illustrative-albedo",
        name: "Illustrative Albedo",
        wire_model: "illustrative-albedo",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 1584,
            min_h: 32,
            max_h: 1536,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_SDXL,
            max_images: 4,
        },
        styles: STYLE_ALBEDO,
        default_style: "Illustration",
        fields: SDXL_FIELDS_FAST_MEDIUM,
    },
    ModelDesc {
        id: "leonardo-concept-art",
        name: "Concept Art",
        wire_model: "concept-art",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 1584,
            min_h: 32,
            max_h: 1536,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_SDXL,
            max_images: 4,
        },
        styles: STYLE_CONCEPT,
        default_style: "726585f5-d231-4a3a-bee9-9a5e5d3df779",
        fields: SDXL_FIELDS_FAST_MEDIUM,
    },
    ModelDesc {
        id: "leonardo-lifelike-vision",
        name: "Lifelike Vision",
        wire_model: "lifelike-vision",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 1584,
            min_h: 32,
            max_h: 1536,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_SDXL,
            max_images: 4,
        },
        styles: STYLE_LIFELIKE,
        default_style: "Dynamic",
        fields: SDXL_FIELDS_FAST_MEDIUM,
    },
    ModelDesc {
        id: "leonardo-graphic-design",
        name: "Graphic Design",
        wire_model: "graphic-design",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 1584,
            min_h: 32,
            max_h: 1536,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_SDXL,
            max_images: 4,
        },
        styles: STYLE_GRAPHIC,
        default_style: "Graphic Design 2D",
        fields: SDXL_FIELDS_FAST_MEDIUM,
    },
    ModelDesc {
        id: "leonardo-lightning-xl",
        name: "Leonardo Lightning",
        wire_model: "lightning-xl",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 1584,
            min_h: 32,
            max_h: 1536,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_SDXL,
            max_images: 4,
        },
        styles: STYLE_LIGHTNING,
        default_style: "Dynamic",
        fields: SDXL_FIELDS_FAST_MEDIUM,
    },
    ModelDesc {
        id: "leonardo-flux-dev",
        name: "FLUX Dev",
        wire_model: "flux-dev",
        prompt: PromptRule::Required { max_chars: 2000 },
        dims: DimensionRule::Range {
            min_w: 480,
            max_w: 2048,
            min_h: 480,
            max_h: 2048,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_CONTENT_STYLE,
            max_images: 1,
        },
        styles: STYLE_FLUX,
        default_style: "Dynamic",
        fields: &[
            ENHANCE_AUTO,
            STYLE,
            seed(2_147_483_637),
            guidance_type(SLOTS_CONTENT_STYLE, "content"),
            GUIDANCE_STRENGTH,
        ],
    },
    ModelDesc {
        id: "leonardo-flux-schnell",
        name: "FLUX Schnell",
        wire_model: "flux-schnell",
        prompt: PromptRule::Required { max_chars: 2000 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 2048,
            min_h: 32,
            max_h: 2048,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::MultiSlot {
            slots: SLOTS_CONTENT_STYLE,
            max_images: 1,
        },
        styles: STYLE_FLUX,
        default_style: "Dynamic",
        fields: &[
            ENHANCE_AUTO,
            STYLE,
            seed(2_147_483_637),
            guidance_type(SLOTS_CONTENT_STYLE, "content"),
            GUIDANCE_STRENGTH,
        ],
    },
    ModelDesc {
        id: "leonardo-flux-kontext-pro",
        name: "FLUX.1 Kontext",
        wire_model: "flux-kontext-pro",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 2048,
            min_h: 32,
            max_h: 2048,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: false,
        },
        guidance: GuidanceMode::ImageReference { max_images: 4 },
        styles: STYLE_KONTEXT,
        default_style: "Dynamic",
        fields: &[ENHANCE_AUTO, STYLE, seed(2_147_483_637)],
    },
    ModelDesc {
        id: "leonardo-flux-kontext-max",
        name: "FLUX.1 Kontext Max",
        wire_model: "flux-kontext-max",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 2048,
            min_h: 32,
            max_h: 2048,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: false,
        },
        guidance: GuidanceMode::ImageReference { max_images: 4 },
        styles: STYLE_KONTEXT,
        default_style: "Dynamic",
        fields: &[ENHANCE_AUTO, STYLE, seed(4_294_967_295)],
    },
    ModelDesc {
        id: "leonardo-flux-pro-2.0",
        name: "FLUX.2 Pro",
        wire_model: "flux-pro-2.0",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 64,
            max_w: 14142,
            min_h: 64,
            max_h: 14142,
            def_w: 1440,
            def_h: 1440,
            multiple_of_8: false,
        },
        guidance: GuidanceMode::ImageReference { max_images: 4 },
        styles: STYLE_KONTEXT,
        default_style: "Dynamic",
        fields: &[ENHANCE_AUTO, STYLE, seed(4_294_967_295)],
    },
    ModelDesc {
        id: "leonardo-flux-3-image",
        name: "FLUX 3 Image",
        wire_model: "bfl/flux-3-image",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Discrete {
            w: FLUX3_DIMS,
            h: FLUX3_DIMS,
            pairs: &[],
            def_w: 1024,
            def_h: 1024,
        },
        guidance: GuidanceMode::ImageReference { max_images: 10 },
        styles: &[],
        default_style: "",
        fields: &[ENHANCE_AUTO],
    },
    ModelDesc {
        id: "leonardo-ideogram-v3.0",
        name: "Ideogram 3.0",
        wire_model: "ideogram-v3.0",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 2048,
            min_h: 32,
            max_h: 2048,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: false,
        },
        guidance: GuidanceMode::None,
        styles: STYLE_LUCID,
        default_style: "Dynamic",
        fields: &[
            opt_sel("quality", QUALITY_THREE),
            ENHANCE_AUTO,
            STYLE,
            NEGATIVE_PROMPT,
            seed(2_147_483_647),
        ],
    },
    ModelDesc {
        id: "leonardo-ideogram-v4.0",
        name: "Ideogram 4.0",
        wire_model: "ideogram-v4.0",
        prompt: PromptRule::Required { max_chars: 10000 },
        dims: DimensionRule::Discrete {
            w: IDEOGRAM_V4_DIMS,
            h: IDEOGRAM_V4_DIMS,
            pairs: IDEOGRAM_V4_PAIRS,
            def_w: 2048,
            def_h: 2048,
        },
        guidance: GuidanceMode::None,
        styles: &[],
        default_style: "",
        fields: &[sel("quality", QUALITY_THREE, "BALANCED"), ENHANCE_AUTO],
    },
    ModelDesc {
        id: "leonardo-p-image-ideogram",
        name: "P-Image-Ideogram",
        wire_model: "ideogram/p-image-ideogram",
        prompt: PromptRule::Required { max_chars: 10000 },
        dims: DimensionRule::Discrete {
            w: P_IMAGE_DIMS,
            h: P_IMAGE_DIMS,
            pairs: &[],
            def_w: 1024,
            def_h: 1024,
        },
        guidance: GuidanceMode::None,
        styles: &[],
        default_style: "",
        fields: &[sel("quality", QUALITY_FOUR, "MEDIUM"), ENHANCE_AUTO],
    },
    ModelDesc {
        id: "leonardo-ideogram-4.5",
        name: "Ideogram 4.5",
        wire_model: "ideogram/ideogram-4.5",
        prompt: PromptRule::Required { max_chars: 10000 },
        dims: DimensionRule::Discrete {
            w: IDEOGRAM_45_DIMS,
            h: IDEOGRAM_45_DIMS,
            pairs: &[],
            def_w: 1024,
            def_h: 1024,
        },
        guidance: GuidanceMode::ImageReference { max_images: 5 },
        styles: &[],
        default_style: "",
        fields: &[
            sel("quality", QUALITY_FOUR, "MEDIUM"),
            ENHANCE_AUTO,
            seed(2_147_483_647),
        ],
    },
    ModelDesc {
        id: "leonardo-gemini-2.5-flash-image",
        name: "Nano Banana",
        wire_model: "gemini-2.5-flash-image",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Discrete {
            w: GEMINI25_W_DIMS,
            h: GEMINI25_H_DIMS,
            pairs: GEMINI_25_PAIRS,
            def_w: 1024,
            def_h: 1024,
        },
        guidance: GuidanceMode::ImageReference { max_images: 6 },
        styles: STYLE_KONTEXT,
        default_style: "Dynamic",
        fields: &[ENHANCE_AUTO, STYLE, seed(2_147_483_638)],
    },
    ModelDesc {
        id: "leonardo-nano-banana-2",
        name: "Nano Banana 2",
        wire_model: "nano-banana-2",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Discrete {
            w: NANO2_W_DIMS,
            h: NANO2_H_DIMS,
            pairs: NANO2_PAIRS,
            def_w: 1024,
            def_h: 1024,
        },
        guidance: GuidanceMode::ImageReference { max_images: 6 },
        styles: STYLE_KONTEXT,
        default_style: "Dynamic",
        fields: &[ENHANCE_AUTO, STYLE, seed(2_147_483_638)],
    },
    ModelDesc {
        id: "leonardo-nano-banana-2.1",
        name: "Nano Banana 2.1",
        wire_model: "google/nano-banana-2.1",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Discrete {
            w: NANO2_W_DIMS,
            h: NANO2_H_DIMS,
            pairs: NANO21_PAIRS,
            def_w: 1024,
            def_h: 1024,
        },
        guidance: GuidanceMode::ImageReference { max_images: 14 },
        styles: STYLE_KONTEXT,
        default_style: "Dynamic",
        fields: &[ENHANCE_AUTO, STYLE],
    },
    ModelDesc {
        id: "leonardo-nano-banana-2-lite",
        name: "Nano Banana 2 Lite",
        wire_model: "nano-banana-2-lite",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Discrete {
            w: NANO_LITE_W_DIMS,
            h: NANO_LITE_H_DIMS,
            pairs: &[],
            def_w: 1024,
            def_h: 1024,
        },
        guidance: GuidanceMode::ImageReference { max_images: 6 },
        styles: STYLE_KONTEXT,
        default_style: "Dynamic",
        fields: &[ENHANCE_AUTO, STYLE, seed(2_147_483_638)],
    },
    ModelDesc {
        id: "leonardo-gemini-image-2",
        name: "Nano Banana Pro",
        wire_model: "gemini-image-2",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Discrete {
            w: NANO2_W_DIMS,
            h: NANO2_H_DIMS,
            pairs: &[],
            def_w: 1024,
            def_h: 1024,
        },
        guidance: GuidanceMode::ImageReference { max_images: 6 },
        styles: STYLE_KONTEXT,
        default_style: "Dynamic",
        fields: &[ENHANCE_AUTO, STYLE, seed(2_147_483_638)],
    },
    ModelDesc {
        id: "leonardo-seedream-4.0",
        name: "Seedream 4.0",
        wire_model: "seedream-4.0",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 14142,
            min_h: 32,
            max_h: 14142,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: false,
        },
        guidance: GuidanceMode::ImageReference { max_images: 6 },
        styles: STYLE_SEEDREAM,
        default_style: "Dynamic",
        fields: &[ENHANCE_AUTO, STYLE, seed(2_147_483_638)],
    },
    ModelDesc {
        id: "leonardo-seedream-4.5",
        name: "Seedream 4.5",
        wire_model: "seedream-4.5",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 32,
            max_w: 14142,
            min_h: 32,
            max_h: 14142,
            def_w: 2048,
            def_h: 2048,
            multiple_of_8: false,
        },
        guidance: GuidanceMode::ImageReference { max_images: 6 },
        styles: STYLE_SEEDREAM,
        default_style: "Dynamic",
        fields: &[ENHANCE_AUTO, STYLE, seed(2_147_483_638)],
    },
    ModelDesc {
        id: "leonardo-seedream-5.0-pro",
        name: "Seedream 5.0 Pro",
        wire_model: "seedream-5.0-pro",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 768,
            max_w: 2048,
            min_h: 768,
            max_h: 2048,
            def_w: 2048,
            def_h: 2048,
            multiple_of_8: false,
        },
        guidance: GuidanceMode::ImageReference { max_images: 10 },
        styles: &[],
        default_style: "",
        fields: &[
            sel("mode", MODE_FAST_QUALITY, "QUALITY"),
            sel("output_format", OUTPUT_FORMATS, "jpeg"),
            ENHANCE_AUTO,
            seed(2_147_483_637),
        ],
    },
    ModelDesc {
        id: "leonardo-seedream-5.0-flash",
        name: "Seedream 5.0 Flash",
        wire_model: "bytedance/seedream-5.0-flash",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 1024,
            max_w: 3008,
            min_h: 1024,
            max_h: 3008,
            def_w: 2048,
            def_h: 2048,
            multiple_of_8: false,
        },
        guidance: GuidanceMode::ImageReference { max_images: 10 },
        styles: &[],
        default_style: "",
        fields: &[sel("output_format", OUTPUT_FORMATS, "jpeg"), ENHANCE_AUTO],
    },
    ModelDesc {
        id: "leonardo-gpt-image-1.5",
        name: "GPT Image-1.5",
        wire_model: "gpt-image-1.5",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 1024,
            max_w: 1536,
            min_h: 1024,
            max_h: 1536,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::ImageReference { max_images: 6 },
        styles: STYLE_GPT,
        default_style: "Dynamic",
        fields: &[
            sel("quality", QUALITY_GPT, "MEDIUM"),
            TRANSPARENCY,
            ENHANCE_AUTO,
            STYLE,
        ],
    },
    ModelDesc {
        id: "leonardo-gpt-image-2",
        name: "GPT Image 2",
        wire_model: "gpt-image-2",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Discrete {
            w: GPT_IMAGE_W_DIMS,
            h: GPT_IMAGE_H_DIMS,
            pairs: &[],
            def_w: 1024,
            def_h: 1024,
        },
        guidance: GuidanceMode::ImageReference { max_images: 6 },
        styles: STYLE_GPT,
        default_style: "Dynamic",
        fields: &[sel("quality", QUALITY_GPT, "MEDIUM"), ENHANCE_AUTO, STYLE],
    },
    ModelDesc {
        id: "leonardo-gpt-image-2.5-flare",
        name: "GPT Image 2.5 Flare",
        wire_model: "openai/gpt-image-2.5-flare",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Discrete {
            w: GPT_IMAGE_W_DIMS,
            h: GPT_IMAGE_H_DIMS,
            pairs: &[],
            def_w: 1024,
            def_h: 1024,
        },
        guidance: GuidanceMode::ImageReference { max_images: 16 },
        styles: STYLE_GPT,
        default_style: "Dynamic",
        fields: &[
            sel("quality", QUALITY_GPT_MAX, "HIGH"),
            TRANSPARENCY,
            ENHANCE_AUTO,
            STYLE,
        ],
    },
    ModelDesc {
        id: "leonardo-gpt-image-2.5-sunburst",
        name: "GPT Image 2.5 Sunburst",
        wire_model: "openai/gpt-image-2.5-sunburst",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Discrete {
            w: GPT_IMAGE_W_DIMS,
            h: GPT_IMAGE_H_DIMS,
            pairs: &[],
            def_w: 1024,
            def_h: 1024,
        },
        guidance: GuidanceMode::ImageReference { max_images: 16 },
        styles: STYLE_GPT,
        default_style: "Dynamic",
        fields: &[
            sel("quality", QUALITY_GPT_MAX, "HIGH"),
            TRANSPARENCY,
            ENHANCE_AUTO,
            STYLE,
        ],
    },
    ModelDesc {
        id: "leonardo-recraft-v4",
        name: "Recraft V4",
        wire_model: "recraft-v4",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Discrete {
            w: RECRAFT_DIMS,
            h: RECRAFT_DIMS,
            pairs: &[],
            def_w: 1024,
            def_h: 1024,
        },
        guidance: GuidanceMode::None,
        styles: &[],
        default_style: "",
        fields: &[ENHANCE_AUTO],
    },
    ModelDesc {
        id: "leonardo-recraft-v4-pro",
        name: "Recraft V4 Pro",
        wire_model: "recraft-v4-pro",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Discrete {
            w: RECRAFT_PRO_DIMS,
            h: RECRAFT_PRO_DIMS,
            pairs: &[],
            def_w: 2048,
            def_h: 2048,
        },
        guidance: GuidanceMode::None,
        styles: &[],
        default_style: "",
        fields: &[ENHANCE_AUTO],
    },
    ModelDesc {
        id: "leonardo-krea-2-turbo",
        name: "Krea 2 Turbo",
        wire_model: "krea-2-turbo",
        prompt: PromptRule::Required { max_chars: 9999 },
        dims: DimensionRule::Range {
            min_w: 16,
            max_w: 4096,
            min_h: 16,
            max_h: 4096,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::None,
        styles: &[],
        default_style: "",
        fields: &[ENHANCE_AUTO],
    },
    ModelDesc {
        id: "leonardo-krea-2",
        name: "Krea 2",
        wire_model: "krea/krea-2",
        prompt: PromptRule::Required { max_chars: 5000 },
        dims: DimensionRule::Range {
            min_w: 16,
            max_w: 4096,
            min_h: 16,
            max_h: 4096,
            def_w: 1024,
            def_h: 1024,
            multiple_of_8: true,
        },
        guidance: GuidanceMode::KreaStyle,
        styles: &[],
        default_style: "",
        fields: &[
            sel("quality", QUALITY_KREA, "MEDIUM"),
            ENHANCE_AUTO,
            seed(4_294_967_295),
            GUIDANCE_STRENGTH,
        ],
    },
    ModelDesc {
        id: "leonardo-aurora-upscaler-creative",
        name: "Aurora Upscaler Creative",
        wire_model: "aurora-upscaler-creative",
        prompt: PromptRule::Unused,
        dims: DimensionRule::FromInputImage { min_side: 128 },
        guidance: GuidanceMode::ImageReference { max_images: 1 },
        styles: &[],
        default_style: "",
        fields: &[
            Extra::UpscaleFactor,
            sel("creativity", CREATIVITY, "mid"),
            seed(2_147_483_647),
        ],
    },
    ModelDesc {
        id: "leonardo-aurora-upscaler-precise",
        name: "Aurora Upscaler Precise",
        wire_model: "aurora-upscaler-precise",
        prompt: PromptRule::Unused,
        dims: DimensionRule::FromInputImage { min_side: 128 },
        guidance: GuidanceMode::ImageReference { max_images: 1 },
        styles: &[],
        default_style: "",
        fields: &[
            Extra::UpscaleFactor,
            sel("upscale_mode", UPSCALE_MODES, "detailed"),
            seed(2_147_483_647),
        ],
    },
    ModelDesc {
        id: "leonardo-remove-bg",
        name: "Remove Background",
        wire_model: "remove-bg",
        prompt: PromptRule::Unused,
        dims: DimensionRule::None,
        guidance: GuidanceMode::RemoveBg,
        styles: &[],
        default_style: "",
        fields: &[
            sel("size", BG_SIZES, "full"),
            sel("type", BG_TYPES, "auto"),
            opt_sel("type_level", BG_TYPE_LEVELS),
            sel("format", BG_FORMATS, "png"),
            sel("channels", BG_CHANNELS, "rgba"),
            boolean("crop", Some(false)),
            text("crop_margin"),
            text("scale"),
            text("position"),
            text("roi"),
            text("bg_color"),
            sel("shadow_type", BG_SHADOW_TYPES, "none"),
            Extra::Int {
                key: "shadow_opacity",
                min: 0,
                max: 100,
                default: None,
            },
            boolean("semitransparency", Some(true)),
        ],
    },
];

static SDXL_FIELDS_FAST_MEDIUM: &[Extra] = &[
    sel("mode", MODE_FAST_QUALITY, "FAST"),
    CONTRAST_MEDIUM,
    ENHANCE_OFF,
    STYLE,
    NEGATIVE_PROMPT,
    TRANSPARENCY_FALSE,
    TILING,
    seed(4_294_967_295),
    guidance_type(SLOTS_SDXL, "image_to_image"),
    GUIDANCE_STRENGTH,
];
static SDXL_FIELDS_FAST_LOW: &[Extra] = &[
    sel("mode", MODE_FAST_QUALITY, "FAST"),
    CONTRAST_LOW,
    ENHANCE_OFF,
    STYLE,
    NEGATIVE_PROMPT,
    TRANSPARENCY_FALSE,
    TILING,
    seed(4_294_967_295),
    guidance_type(SLOTS_SDXL, "image_to_image"),
    GUIDANCE_STRENGTH,
];
static SDXL_FIELDS_QUALITY_MEDIUM: &[Extra] = &[
    sel("mode", MODE_FAST_QUALITY, "QUALITY"),
    CONTRAST_MEDIUM,
    ENHANCE_OFF,
    STYLE,
    NEGATIVE_PROMPT,
    TRANSPARENCY_FALSE,
    TILING,
    seed(4_294_967_295),
    guidance_type(SLOTS_SDXL, "image_to_image"),
    GUIDANCE_STRENGTH,
];

static SPECS: LazyLock<[ModelSpec; 43]> =
    LazyLock::new(|| std::array::from_fn(|index| MODELS[index].build_spec()));

/// The registered Leonardo model specs, in the order of the model table.
pub fn specs() -> impl Iterator<Item = &'static ModelSpec> {
    SPECS.iter()
}

/// The provider that owns a registered Leonardo model id.
#[must_use]
pub fn resolve(model_id: &str) -> Option<LeonardoModel> {
    MODELS
        .iter()
        .find(|desc| desc.id == model_id)
        .map(LeonardoModel)
}

fn spec_for(desc: &ModelDesc) -> &'static ModelSpec {
    let index = MODELS
        .iter()
        .position(|candidate| candidate.id == desc.id)
        .expect("descriptor is registered");
    &SPECS[index]
}

impl Provider for LeonardoModel {
    fn spec(&self) -> &ModelSpec {
        spec_for(self.0)
    }

    fn validate_for_submit(
        &self,
        scene: &Scene,
        values: &BTreeMap<String, Value>,
        image_count: usize,
    ) -> Vec<FieldError> {
        validate(self.0, self.spec(), scene, values, image_count)
    }

    fn reference_steps(&self, images: &[EncodedImage]) -> Vec<ReferenceStep> {
        images
            .iter()
            .map(|image| ReferenceStep::Inline(ImageInput::RawBase64(image.base64())))
            .collect()
    }

    fn build_submit(&self, request: &JobRequest, images: &[ImageInput]) -> HttpRequest {
        let desc = self.0;
        let mut params = Map::new();
        params.insert("quantity".into(), json!(1));
        if let PromptRule::Required { .. } = desc.prompt {
            params.insert("prompt".into(), json!(request.compiled_prompt));
        }
        match desc.dims {
            DimensionRule::Range { .. } | DimensionRule::Discrete { .. } => {
                for key in ["width", "height"] {
                    if let Some(value) = request.params.get(key) {
                        params.insert(key.into(), value.clone());
                    }
                }
            }
            DimensionRule::FromInputImage { .. } => {
                if let Some(image) = request.images.first() {
                    params.insert("width".into(), json!(image.width));
                    params.insert("height".into(), json!(image.height));
                }
            }
            DimensionRule::None => {}
        }
        for extra in desc.fields {
            let Some(value) = request.params.get(extra.key()) else {
                continue;
            };
            if extra.ui_only() || value.is_null() {
                continue;
            }
            match extra {
                Extra::Select { .. } | Extra::Text { .. } => {
                    if let Some(text) = value.as_str()
                        && !text.is_empty()
                    {
                        params.insert(extra.key().into(), json!(text));
                    }
                }
                Extra::Bool { .. } => {
                    if let Some(flag) = value.as_bool() {
                        params.insert(extra.key().into(), json!(flag));
                    }
                }
                Extra::Seed { .. } => {
                    if let Some(number) = value.as_i64()
                        && number > 0
                    {
                        params.insert(extra.key().into(), json!(number));
                    }
                }
                Extra::Int { .. } => {
                    if let Some(number) = value.as_i64() {
                        params.insert(extra.key().into(), json!(number));
                    }
                }
                Extra::UpscaleFactor => {
                    if let Some(factor) = value.as_str().and_then(|text| text.parse::<i64>().ok()) {
                        params.insert("upscale_factor".into(), json!(factor));
                    }
                }
                Extra::Style => {
                    if let Some(uuid) = value.as_str().and_then(style_uuid) {
                        params.insert("style_ids".into(), json!([uuid]));
                    }
                }
                Extra::GuidanceType { .. } | Extra::GuidanceStrength => {}
            }
        }
        if !images.is_empty()
            && let Some(guidances) = build_guidances(desc, request, images)
        {
            params.insert("guidances".into(), guidances);
        }
        HttpRequest {
            method: Method::Post,
            url: GENERATIONS_URL.into(),
            headers: Vec::new(),
            auth: Some(ProviderId::Leonardo),
            body: Body::Json(json!({
                "model": desc.wire_model,
                "public": false,
                "parameters": Value::Object(params),
            })),
            max_body_bytes: 100 * 1024 * 1024,
            redact_query: false,
        }
    }

    fn parse_submit(&self, response: &HttpResponse) -> Result<Submitted, ProviderError> {
        parse_submit(response)
    }

    fn poll_request(&self, submitted: &Submitted) -> HttpRequest {
        poll_request(submitted)
    }

    fn parse_poll(&self, response: &HttpResponse) -> PollOutcome {
        parse_poll(response)
    }

    fn poll_policy(&self) -> PollPolicy {
        PollPolicy {
            initial: Duration::from_secs(2),
            factor: 1.25,
            cap: Duration::from_secs(5),
            deadline: Duration::from_secs(600),
        }
    }
}

fn build_guidances(desc: &ModelDesc, request: &JobRequest, images: &[ImageInput]) -> Option<Value> {
    let strength = || guidance_strength_of(&request.params).to_owned();
    match desc.guidance {
        GuidanceMode::None => None,
        GuidanceMode::MultiSlot { max_images, .. } => {
            let slot = guidance_type_of(desc, &request.params).to_owned();
            let cap = if slot == "style" { max_images } else { 1 };
            let entries: Vec<Value> = images
                .iter()
                .take(cap)
                .map(|input| strength_entry(input, &strength()))
                .collect();
            if entries.is_empty() {
                return None;
            }
            let mut guidances = Map::new();
            guidances.insert(slot, Value::Array(entries));
            Some(Value::Object(guidances))
        }
        GuidanceMode::KreaStyle => {
            let entries: Vec<Value> = images
                .iter()
                .take(KREA_STYLE_MAX_IMAGES)
                .map(|input| strength_entry(input, &strength()))
                .collect();
            (!entries.is_empty()).then(|| json!({ "style": entries }))
        }
        GuidanceMode::ImageReference { max_images } => {
            let entries: Vec<Value> = images
                .iter()
                .take(max_images)
                .map(|input| json!({ "image": { "type": "BASE64", "data": input.as_str() } }))
                .collect();
            (!entries.is_empty()).then(|| json!({ "image_reference": entries }))
        }
        GuidanceMode::RemoveBg => {
            let mut guidances = Map::new();
            if let Some(input) = images.first() {
                guidances.insert(
                    "image_reference".into(),
                    json!([{ "image": { "type": "BASE64", "data": input.as_str() } }]),
                );
            }
            if let Some(input) = images.get(1) {
                guidances.insert(
                    "background_image_reference".into(),
                    json!([{ "image": { "type": "BASE64", "data": input.as_str() } }]),
                );
            }
            (!guidances.is_empty()).then_some(Value::Object(guidances))
        }
    }
}

fn strength_entry(input: &ImageInput, strength: &str) -> Value {
    json!({
        "image": { "type": "BASE64", "data": input.as_str() },
        "strength": strength,
    })
}

fn validate(
    desc: &ModelDesc,
    spec: &ModelSpec,
    scene: &Scene,
    values: &BTreeMap<String, Value>,
    image_count: usize,
) -> Vec<FieldError> {
    let ctx = RuleContext {
        has_source_image: scene.source_image.is_some(),
        image_count,
        box_count: scene.boxes.len(),
    };
    let mut errors = spec.schema.validate(values, &ctx);
    match desc.prompt {
        PromptRule::Required { max_chars } => {
            if scene.prose.trim().is_empty() {
                errors.push(FieldError {
                    key: "prompt".into(),
                    message: "Prompt is empty".into(),
                    approved_max: None,
                });
            } else if scene.prose.chars().count() > max_chars {
                errors.push(FieldError {
                    key: "prompt".into(),
                    message: format!("Prompt is over {max_chars} characters."),
                    approved_max: None,
                });
            }
        }
        PromptRule::Unused => {
            if image_count == 0 {
                errors.push(FieldError {
                    key: "images".into(),
                    message: "Add at least one image.".into(),
                    approved_max: None,
                });
            }
        }
    }
    if let DimensionRule::FromInputImage { min_side } = desc.dims
        && let Some((width, height)) = first_slot(scene)
    {
        let (prep_w, prep_h) = crate::image_prep::fit_pixels(width, height, 4_000_000);
        if prep_w < min_side || prep_h < min_side {
            errors.push(FieldError {
                key: "images".into(),
                message: format!("Input image must be at least {min_side}×{min_side}."),
                approved_max: None,
            });
        } else {
            let factor = values
                .get("upscale_factor")
                .and_then(Value::as_str)
                .and_then(|text| text.parse::<i64>().ok())
                .unwrap_or(2);
            let out_w = i64::from(prep_w) * factor;
            let out_h = i64::from(prep_h) * factor;
            if out_w > 16_384 || out_h > 16_384 || out_w * out_h > 64_000_000 {
                errors.push(FieldError {
                    key: "images".into(),
                    message: "Upscaled image exceeds 16384px or 64 MP limit.".into(),
                    approved_max: None,
                });
            }
        }
    }
    if let DimensionRule::Discrete { w, h, pairs, .. } = desc.dims {
        let width = values.get("width").and_then(Value::as_i64);
        let height = values.get("height").and_then(Value::as_i64);
        if let (Some(width), Some(height)) = (width, height) {
            if !w.contains(&width) {
                errors.push(FieldError {
                    key: "width".into(),
                    message: format!("Width must be one of {}", dim_list(w)),
                    approved_max: None,
                });
            }
            if !h.contains(&height) {
                errors.push(FieldError {
                    key: "height".into(),
                    message: format!("Height must be one of {}", dim_list(h)),
                    approved_max: None,
                });
            }
            if !pairs.is_empty() && !pairs.contains(&(width, height)) {
                errors.push(FieldError {
                    key: "width".into(),
                    message: format!(
                        "Width and height must form one of the supported pairs: {}",
                        pair_list(pairs)
                    ),
                    approved_max: None,
                });
            }
        }
    }
    if image_count >= 1 {
        match desc.guidance {
            GuidanceMode::MultiSlot { .. } => {
                let slot = guidance_type_of(desc, values);
                let strength = guidance_strength_of(values);
                let (allowed, message) = match slot {
                    "style" => (
                        STYLE_STRENGTHS,
                        format!(
                            "Style guidance strength must be {}.",
                            or_list(STYLE_STRENGTHS)
                        ),
                    ),
                    "content" => (
                        CONTENT_STRENGTHS,
                        format!(
                            "Content guidance strength must be {}.",
                            or_list(CONTENT_STRENGTHS)
                        ),
                    ),
                    other => (
                        BASIC_STRENGTHS,
                        format!(
                            "Guidance strength for {other} must be {}.",
                            or_list(BASIC_STRENGTHS)
                        ),
                    ),
                };
                if !allowed.contains(&strength) {
                    errors.push(FieldError {
                        key: "guidance_strength".into(),
                        message,
                        approved_max: None,
                    });
                }
            }
            GuidanceMode::KreaStyle => {
                let strength = guidance_strength_of(values);
                if !STYLE_STRENGTHS.contains(&strength) {
                    errors.push(FieldError {
                        key: "guidance_strength".into(),
                        message: format!(
                            "Guidance strength must be {}.",
                            STYLE_STRENGTHS.join(", ")
                        ),
                        approved_max: None,
                    });
                }
            }
            GuidanceMode::None | GuidanceMode::ImageReference { .. } | GuidanceMode::RemoveBg => {}
        }
    }
    errors
}

fn first_slot(scene: &Scene) -> Option<(u32, u32)> {
    scene.slots().first().map(|slot| (slot.width, slot.height))
}

fn or_list(values: &[&str]) -> String {
    match values.len() {
        0 => String::new(),
        1 => values[0].to_owned(),
        n => format!("{}, or {}", values[..n - 1].join(", "), values[n - 1]),
    }
}

fn dim_list(values: &[i64]) -> String {
    values
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn pair_list(pairs: &[(i64, i64)]) -> String {
    pairs
        .iter()
        .map(|(w, h)| format!("{w}×{h}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_submit(response: &HttpResponse) -> Result<Submitted, ProviderError> {
    if response.status != 200 {
        return Err(parse_error(response, true));
    }
    let value = json_body(&response.body)?;
    let id = value
        .get("generationId")
        .or_else(|| value.pointer("/generate/generationId"))
        .or_else(|| value.pointer("/sdGenerationJob/generationId"))
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::Protocol("submit response had no generationId".into()))?;
    let job_id = JobId::parse(id)
        .map_err(|_| ProviderError::Protocol("submit response had no generationId".into()))?;
    let poll_url = url::Url::parse(&format!("{GENERATION_URL}/{}", job_id.as_str()))
        .map_err(|_| ProviderError::Protocol("poll url was not valid".into()))?;
    Ok(Submitted {
        job_id,
        poll_url,
        quote: None,
    })
}

fn poll_request(submitted: &Submitted) -> HttpRequest {
    HttpRequest {
        method: Method::Get,
        url: submitted.poll_url.to_string(),
        headers: Vec::new(),
        auth: Some(ProviderId::Leonardo),
        body: Body::Empty,
        max_body_bytes: 2 * 1024 * 1024,
        redact_query: false,
    }
}

fn parse_poll(response: &HttpResponse) -> PollOutcome {
    if response.status != 200 {
        return PollOutcome::Failed(parse_error(response, false));
    }
    let Ok(value) = serde_json::from_slice::<Value>(&response.body) else {
        return PollOutcome::Failed(ProviderError::Protocol("poll body was not json".into()));
    };
    let Some(pk) = value.get("generations_by_pk").filter(|pk| !pk.is_null()) else {
        return PollOutcome::Failed(ProviderError::Protocol(
            "poll response missing generations_by_pk".into(),
        ));
    };
    let Some(status) = pk.get("status").and_then(Value::as_str) else {
        return PollOutcome::Failed(ProviderError::Protocol(
            "poll response had no status".into(),
        ));
    };
    match status {
        "PENDING" | "QUEUED" | "PROCESSING" => PollOutcome::InFlight {
            status: status.to_owned(),
            progress: None,
        },
        "FAILED" => PollOutcome::Failed(ProviderError::Protocol("generation failed".into())),
        "COMPLETE" => complete_outcome(pk),
        other => PollOutcome::Failed(ProviderError::Protocol(format!(
            "unexpected status {other}"
        ))),
    }
}

fn complete_outcome(pk: &Value) -> PollOutcome {
    let images = pk
        .get("generated_images")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let Some(first) = images.first() else {
        return PollOutcome::Failed(ProviderError::NoImage);
    };
    if first.get("nsfw").and_then(Value::as_bool) == Some(true) {
        return PollOutcome::Failed(ProviderError::Blocked);
    }
    let Some(url) = first.get("url").and_then(Value::as_str) else {
        return PollOutcome::Failed(ProviderError::Protocol(
            "completed poll had no image url".into(),
        ));
    };
    if !ProviderId::Leonardo.allows_url(url) {
        return PollOutcome::Failed(ProviderError::Protocol("unexpected result host".into()));
    }
    let Ok(url) = url::Url::parse(url) else {
        return PollOutcome::Failed(ProviderError::Protocol("unexpected result host".into()));
    };
    PollOutcome::Ready {
        url,
        settled_cost: None,
        prompt: None,
        duration: None,
        grid_urls: None,
    }
}

fn parse_error(response: &HttpResponse, submit: bool) -> ProviderError {
    match response.status {
        401 | 403 => ProviderError::Auth,
        402 => ProviderError::OutOfCredits { hint: None },
        404 if submit => ProviderError::ModelNotFound,
        404 => ProviderError::Protocol("generation not found".into()),
        429 => ProviderError::RateLimited { retry_after: None },
        400 | 422 => {
            let text = error_text(&response.body);
            let lowered = text.to_ascii_lowercase();
            if lowered.contains("content moderation") || lowered.contains("nsfw") {
                ProviderError::Blocked
            } else if text.is_empty() {
                ProviderError::Http {
                    status: response.status,
                    body: cap_text(&String::from_utf8_lossy(&response.body)),
                }
            } else {
                ProviderError::Validation {
                    message: cap_text(&text),
                    field_errors: Vec::new(),
                }
            }
        }
        status => ProviderError::Http {
            status,
            body: cap_text(&String::from_utf8_lossy(&response.body)),
        },
    }
}

fn error_text(body: &[u8]) -> String {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return String::new();
    };
    if let Some(text) = value.get("error").and_then(Value::as_str) {
        return text.to_owned();
    }
    if let Some(text) = value.pointer("/error/message").and_then(Value::as_str) {
        return text.to_owned();
    }
    if let Some(text) = value.get("message").and_then(Value::as_str) {
        return text.to_owned();
    }
    String::new()
}

fn json_body(body: &[u8]) -> Result<Value, ProviderError> {
    serde_json::from_slice(body)
        .map_err(|_| ProviderError::Protocol("response body was not json".into()))
}

fn cap_text(text: &str) -> String {
    crate::text::cap_text(text, STORED_CAP)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use serde_json::{Value, json};

    use super::{GENERATIONS_URL, GuidanceMode, MODELS, PromptRule, style_label, style_uuid};
    use crate::error::ProviderError;
    use crate::http::HttpResponse;
    use crate::job::{EncodedImage, JobRequest};
    use crate::model_spec::ProviderId;
    use crate::providers::{ModelRegistry, PollOutcome, Provider};
    use crate::scene::{ImageRef, Scene};

    fn provider(id: &str) -> super::LeonardoModel {
        super::resolve(id).unwrap_or_else(|| panic!("unregistered model {id}"))
    }

    fn values(provider: &super::LeonardoModel) -> BTreeMap<String, Value> {
        provider.spec().schema.defaults()
    }

    fn scene_with(prose: &str) -> Scene {
        Scene {
            prose: prose.into(),
            ..Scene::empty()
        }
    }

    fn encoded(width: u32, height: u32) -> EncodedImage {
        EncodedImage {
            media_type: "image/png".into(),
            bytes: b"PNG".to_vec(),
            width,
            height,
            content_hash: "hash".into(),
        }
    }

    fn request(
        provider: &super::LeonardoModel,
        prompt: &str,
        values: BTreeMap<String, Value>,
        images: Vec<EncodedImage>,
    ) -> JobRequest {
        JobRequest {
            model_id: provider.spec().id.clone(),
            compiled_prompt: prompt.into(),
            params: values,
            images,
            box_snapshot: Vec::new(),
            created_at: "2026-10-25T00:00:00Z".into(),
        }
    }

    fn body_of(provider: &super::LeonardoModel, request: &JobRequest) -> Value {
        let images: Vec<crate::providers::ImageInput> = request
            .images
            .iter()
            .map(|image| crate::providers::ImageInput::RawBase64(image.base64()))
            .collect();
        let http = provider.build_submit(request, &images);
        assert_eq!(http.url, GENERATIONS_URL);
        assert_eq!(http.auth, Some(ProviderId::Leonardo));
        let crate::http::Body::Json(body) = http.body else {
            panic!("submit body was not json");
        };
        body
    }

    fn parameters(provider: &super::LeonardoModel, request: &JobRequest) -> Value {
        body_of(provider, request)["parameters"].clone()
    }

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    const MODEL_ORDER: &[&str] = &[
        "leonardo-phoenix-v1.0",
        "leonardo-phoenix-v0.9",
        "leonardo-lucid-origin",
        "leonardo-lucid-realism",
        "leonardo-kino-xl",
        "leonardo-anime-xl",
        "leonardo-portrait-perfect",
        "leonardo-stock-photography",
        "leonardo-illustrative-albedo",
        "leonardo-concept-art",
        "leonardo-lifelike-vision",
        "leonardo-graphic-design",
        "leonardo-lightning-xl",
        "leonardo-flux-dev",
        "leonardo-flux-schnell",
        "leonardo-flux-kontext-pro",
        "leonardo-flux-kontext-max",
        "leonardo-flux-pro-2.0",
        "leonardo-flux-3-image",
        "leonardo-ideogram-v3.0",
        "leonardo-ideogram-v4.0",
        "leonardo-p-image-ideogram",
        "leonardo-ideogram-4.5",
        "leonardo-gemini-2.5-flash-image",
        "leonardo-nano-banana-2",
        "leonardo-nano-banana-2.1",
        "leonardo-nano-banana-2-lite",
        "leonardo-gemini-image-2",
        "leonardo-seedream-4.0",
        "leonardo-seedream-4.5",
        "leonardo-seedream-5.0-pro",
        "leonardo-seedream-5.0-flash",
        "leonardo-gpt-image-1.5",
        "leonardo-gpt-image-2",
        "leonardo-gpt-image-2.5-flare",
        "leonardo-gpt-image-2.5-sunburst",
        "leonardo-recraft-v4",
        "leonardo-recraft-v4-pro",
        "leonardo-krea-2-turbo",
        "leonardo-krea-2",
        "leonardo-aurora-upscaler-creative",
        "leonardo-aurora-upscaler-precise",
        "leonardo-remove-bg",
    ];

    #[test]
    fn the_registry_lists_all_forty_three_models_in_table_order() {
        let ids: Vec<&str> = ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.provider == ProviderId::Leonardo)
            .map(|spec| spec.id.as_str())
            .collect();
        assert_eq!(ids, MODEL_ORDER);
        for id in MODEL_ORDER {
            let resolved = crate::providers::resolve_model(id).expect("resolvable");
            assert_eq!(resolved.spec().provider, ProviderId::Leonardo);
        }
        let gpt: Vec<_> = ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.id.starts_with("gpt-image"))
            .collect();
        assert_eq!(gpt.len(), 6);
    }

    #[test]
    fn every_model_builds_a_valid_default_payload() {
        for desc in MODELS.iter() {
            let model = provider(desc.id);
            let mut params = values(&model);
            if matches!(desc.prompt, PromptRule::Required { .. }) {
                params.insert("prompt".into(), json!("a fox"));
            }
            let req = request(&model, "a fox", params, Vec::new());
            let body = body_of(&model, &req);
            assert_eq!(body["model"], json!(desc.wire_model), "{}", desc.id);
            assert_eq!(body["public"], json!(false), "{}", desc.id);
            assert_eq!(body["parameters"]["quantity"], json!(1), "{}", desc.id);
            for ui_key in ["guidance_type", "guidance_strength", "style"] {
                assert!(
                    body["parameters"].get(ui_key).is_none(),
                    "{} leaked {}",
                    desc.id,
                    ui_key
                );
            }
            assert!(
                body["parameters"].get("prompt").is_some()
                    == matches!(desc.prompt, PromptRule::Required { .. }),
                "{}",
                desc.id
            );
        }
    }

    #[test]
    fn every_text_model_requires_width_and_height_defaults() {
        for desc in MODELS.iter() {
            let model = provider(desc.id);
            let spec = model.spec();
            let expects_dims = !matches!(
                desc.dims,
                super::DimensionRule::FromInputImage { .. } | super::DimensionRule::None
            );
            let has_width = spec.schema.fields.iter().any(|field| field.key == "width");
            assert_eq!(has_width, expects_dims, "{}", desc.id);
            if has_width {
                let width = spec
                    .schema
                    .fields
                    .iter()
                    .find(|field| field.key == "width")
                    .unwrap();
                assert!(matches!(
                    width.kind,
                    crate::schema::FieldKind::Int { step: 1, .. }
                ));
                let span = match width.kind {
                    crate::schema::FieldKind::Int { min, max, .. } => max - min,
                    _ => 0,
                };
                assert!(span > 200, "{} renders a slider", desc.id);
            }
        }
    }

    #[test]
    fn phoenix_zero_images_sends_style_ids_without_guidances() {
        let model = provider("leonardo-phoenix-v1.0");
        let params = values(&model);
        let req = request(&model, "ancient observatory", params, Vec::new());
        let params_out = parameters(&model, &req);
        assert!(params_out.get("guidances").is_none());
        assert_eq!(
            params_out["style_ids"],
            json!(["111dc692-d470-4eec-b791-3475abac4c46"])
        );
        assert_eq!(params_out["width"], json!(1024));
        assert_eq!(params_out["mode"], json!("FAST"));
        assert_eq!(params_out["contrast"], json!("MEDIUM"));
        assert_eq!(params_out["prompt_enhance"], json!("AUTO"));
        assert!(params_out.get("seed").is_none());
        assert!(params_out.get("negative_prompt").is_none());
        assert!(params_out.get("tiling").is_none());
    }

    #[test]
    fn phoenix_style_guidance_sends_base64_entries_with_strength() {
        let model = provider("leonardo-phoenix-v1.0");
        let mut params = values(&model);
        params.insert("guidance_type".into(), json!("style"));
        params.insert("guidance_strength".into(), json!("MAX"));
        let req = request(
            &model,
            "ancient observatory",
            params,
            vec![encoded(640, 480), encoded(320, 240)],
        );
        let params_out = parameters(&model, &req);
        let entries = params_out["guidances"]["style"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        for entry in entries {
            assert_eq!(entry["image"]["type"], json!("BASE64"));
            assert_eq!(entry["strength"], json!("MAX"));
        }
    }

    #[test]
    fn phoenix_image_to_image_sends_only_the_first_image() {
        let model = provider("leonardo-phoenix-v1.0");
        let mut params = values(&model);
        params.insert("guidance_type".into(), json!("image_to_image"));
        params.insert("guidance_strength".into(), json!("HIGH"));
        let req = request(
            &model,
            "ancient observatory",
            params,
            vec![encoded(640, 480), encoded(320, 240)],
        );
        let params_out = parameters(&model, &req);
        let entries = params_out["guidances"]["image_to_image"]
            .as_array()
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["strength"], json!("HIGH"));
    }

    #[test]
    fn phoenix_content_rejects_max_and_character_rejects_ultra() {
        let model = provider("leonardo-phoenix-v1.0");
        let scene = scene_with("ancient observatory");
        let mut params = values(&model);
        params.insert("guidance_type".into(), json!("content"));
        params.insert("guidance_strength".into(), json!("MAX"));
        let errors = model.validate_for_submit(&scene, &params, 1);
        assert!(
            errors.iter().any(|error| error.message
                == "Content guidance strength must be LOW, MID, HIGH, or ULTRA."),
            "{errors:?}"
        );
        params.insert("guidance_strength".into(), json!("HIGH"));
        assert!(model.validate_for_submit(&scene, &params, 1).is_empty());

        let mut params = values(&model);
        params.insert("guidance_type".into(), json!("character"));
        params.insert("guidance_strength".into(), json!("ULTRA"));
        let errors = model.validate_for_submit(&scene, &params, 1);
        assert!(
            errors.iter().any(|error| error.message
                == "Guidance strength for character must be LOW, MID, or HIGH."),
            "{errors:?}"
        );
    }

    #[test]
    fn krea_two_sends_style_guidance_only_with_images() {
        let model = provider("leonardo-krea-2");
        let mut params = values(&model);
        params.insert("guidance_strength".into(), json!("ULTRA"));
        let req = request(
            &model,
            "poster art",
            params.clone(),
            vec![encoded(512, 512), encoded(512, 512)],
        );
        let params_out = parameters(&model, &req);
        let entries = params_out["guidances"]["style"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1]["strength"], json!("ULTRA"));

        let req = request(&model, "poster art", params, Vec::new());
        assert!(parameters(&model, &req).get("guidances").is_none());
    }

    #[test]
    fn aurora_upscalers_validate_and_serialize_from_the_input_image() {
        for id in [
            "leonardo-aurora-upscaler-creative",
            "leonardo-aurora-upscaler-precise",
        ] {
            let model = provider(id);
            let scene = Scene {
                source_image: Some(ImageRef::new("source", 128, 128)),
                ..Scene::empty()
            };

            let no_image = model.validate_for_submit(&Scene::empty(), &values(&model), 0);
            assert!(
                no_image
                    .iter()
                    .any(|error| error.message == "Add at least one image."),
                "{id}: {no_image:?}"
            );

            let mut params = values(&model);
            params.insert("upscale_factor".into(), json!("4"));
            let ok = model.validate_for_submit(&scene, &params, 1);
            assert!(ok.is_empty(), "{id}: {ok:?}");

            let small = Scene {
                source_image: Some(ImageRef::new("source", 64, 64)),
                ..Scene::empty()
            };
            let errors = model.validate_for_submit(&small, &params, 1);
            assert!(
                errors
                    .iter()
                    .any(|error| error.message == "Input image must be at least 128×128."),
                "{id}: {errors:?}"
            );

            let big = Scene {
                source_image: Some(ImageRef::new("source", 1024, 1024)),
                ..Scene::empty()
            };
            let mut params = values(&model);
            params.insert("upscale_factor".into(), json!("8"));
            let errors = model.validate_for_submit(&big, &params, 1);
            assert!(
                errors
                    .iter()
                    .any(|error| error.message == "Upscaled image exceeds 16384px or 64 MP limit."),
                "{id}: {errors:?}"
            );

            // A 10000×400 input fits 4 MP untouched, so its upscale breaks the
            // 16384px per-side cap.
            let wide = Scene {
                source_image: Some(ImageRef::new("source", 10000, 400)),
                ..Scene::empty()
            };
            let mut params = values(&model);
            params.insert("upscale_factor".into(), json!("2"));
            let errors = model.validate_for_submit(&wide, &params, 1);
            assert!(
                errors
                    .iter()
                    .any(|error| error.message == "Upscaled image exceeds 16384px or 64 MP limit."),
                "{id}: {errors:?}"
            );

            let mut params = values(&model);
            params.insert("upscale_factor".into(), json!("4"));
            let req = request(&model, "", params, vec![encoded(128, 128)]);
            let params_out = parameters(&model, &req);
            assert_eq!(params_out["width"], json!(128));
            assert_eq!(params_out["height"], json!(128));
            assert_eq!(params_out["upscale_factor"], json!(4));
            assert!(params_out.get("prompt").is_none());
            assert!(params_out["guidances"]["image_reference"].is_array());
            let entries = params_out["guidances"]["image_reference"]
                .as_array()
                .unwrap();
            assert_eq!(entries.len(), 1);
            assert!(entries[0].get("strength").is_none());
            if id == "leonardo-aurora-upscaler-precise" {
                assert_eq!(params_out["upscale_mode"], json!("detailed"));
                assert!(params_out.get("creativity").is_none());
            } else {
                assert_eq!(params_out["creativity"], json!("mid"));
                assert!(params_out.get("upscale_mode").is_none());
            }
        }
    }

    #[test]
    fn remove_bg_maps_slot_two_to_the_background() {
        let model = provider("leonardo-remove-bg");
        let params = values(&model);

        let one = request(
            &model,
            "leftover prompt",
            params.clone(),
            vec![encoded(512, 512)],
        );
        let params_out = parameters(&model, &one);
        assert!(params_out.get("prompt").is_none());
        assert!(params_out.get("width").is_none());
        assert!(params_out.get("height").is_none());
        assert!(params_out["guidances"]["image_reference"].is_array());
        assert!(
            params_out["guidances"]
                .get("background_image_reference")
                .is_none()
        );
        assert_eq!(params_out["format"], json!("png"));
        assert_eq!(params_out["semitransparency"], json!(true));
        assert_eq!(params_out["size"], json!("full"));

        let two = request(
            &model,
            "leftover prompt",
            params,
            vec![encoded(512, 512), encoded(512, 512)],
        );
        let params_out = parameters(&model, &two);
        assert_eq!(
            params_out["guidances"]["image_reference"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            params_out["guidances"]["background_image_reference"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn remove_bg_requires_an_image_and_ignores_the_prompt() {
        let model = provider("leonardo-remove-bg");
        let errors = model.validate_for_submit(&scene_with("leftover prompt"), &values(&model), 0);
        assert!(
            errors
                .iter()
                .any(|error| error.message == "Add at least one image."),
            "{errors:?}"
        );
    }

    #[test]
    fn multiple_of_eight_is_enforced_by_the_schema() {
        let model = provider("leonardo-phoenix-v1.0");
        let mut params = values(&model);
        params.insert("width".into(), json!(1025));
        let scene = scene_with("a fox");
        let errors = model.validate_for_submit(&scene, &params, 0);
        assert!(
            errors
                .iter()
                .any(|error| error.key == "width" && error.message.contains("multiple of 8")),
            "{errors:?}"
        );
    }

    #[test]
    fn recraft_rejects_off_allowlist_dimensions() {
        let model = provider("leonardo-recraft-v4");
        let mut params = values(&model);
        params.insert("width".into(), json!(1000));
        let errors = model.validate_for_submit(&scene_with("a fox"), &params, 0);
        let error = errors
            .iter()
            .find(|error| error.key == "width")
            .expect("width error");
        assert!(error.message.contains("768"), "{error:?}");
        assert!(error.message.contains("1536"), "{error:?}");

        let mut params = values(&model);
        params.insert("width".into(), json!(1024));
        params.insert("height".into(), json!(1024));
        assert!(
            model
                .validate_for_submit(&scene_with("a fox"), &params, 0)
                .is_empty()
        );
    }

    #[test]
    fn ideogram_v4_enforces_the_documented_pairs() {
        let model = provider("leonardo-ideogram-v4.0");
        let mut params = values(&model);
        params.insert("width".into(), json!(2048));
        params.insert("height".into(), json!(2048));
        assert!(
            model
                .validate_for_submit(&scene_with("a fox"), &params, 0)
                .is_empty()
        );
        params.insert("width".into(), json!(1440));
        params.insert("height".into(), json!(2880));
        assert!(
            model
                .validate_for_submit(&scene_with("a fox"), &params, 0)
                .is_empty()
        );
        params.insert("height".into(), json!(1440));
        let errors = model.validate_for_submit(&scene_with("a fox"), &params, 0);
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("1440×2880")),
            "{errors:?}"
        );
    }

    #[test]
    fn nano_banana_two_accepts_zero_and_rejects_off_allowlist_pairs() {
        let model = provider("leonardo-nano-banana-2");
        for (width, height) in [(0i64, 0i64), (1024, 1024)] {
            let mut params = values(&model);
            params.insert("width".into(), json!(width));
            params.insert("height".into(), json!(height));
            assert!(
                model
                    .validate_for_submit(&scene_with("a fox"), &params, 0)
                    .is_empty(),
                "{width}×{height}"
            );
        }
        let mut params = values(&model);
        params.insert("width".into(), json!(500));
        params.insert("height".into(), json!(500));
        let errors = model.validate_for_submit(&scene_with("a fox"), &params, 0);
        assert!(
            errors.iter().any(|error| error.key == "width"),
            "{errors:?}"
        );
    }

    #[test]
    fn prompts_are_required_within_the_model_cap() {
        let model = provider("leonardo-phoenix-v1.0");
        let errors = model.validate_for_submit(&scene_with("   "), &values(&model), 0);
        assert!(
            errors
                .iter()
                .any(|error| error.message == "Prompt is empty"),
            "{errors:?}"
        );
        let long = "x".repeat(2001);
        let errors = model.validate_for_submit(&scene_with(&long), &values(&model), 0);
        assert!(
            errors
                .iter()
                .any(|error| error.message == "Prompt is over 2000 characters."),
            "{errors:?}"
        );
        let ok = "x".repeat(2000);
        assert!(
            model
                .validate_for_submit(&scene_with(&ok), &values(&model), 0)
                .is_empty()
        );
    }

    #[test]
    fn style_labels_map_to_uuids_and_back() {
        assert_eq!(
            style_uuid("Dynamic"),
            Some("111dc692-d470-4eec-b791-3475abac4c46")
        );
        assert_eq!(
            style_label("111dc692-d470-4eec-b791-3475abac4c46"),
            "Dynamic"
        );
        let raw = "726585f5-d231-4a3a-bee9-9a5e5d3df779";
        assert_eq!(style_uuid(raw), Some(raw));
        assert_eq!(style_label(raw), raw);
        assert_eq!(style_uuid("Not a style"), None);
    }

    #[test]
    fn style_dropdown_options_match_each_models_uuid_list() {
        for desc in MODELS.iter() {
            let model = provider(desc.id);
            let Some(field) = model
                .spec()
                .schema
                .fields
                .iter()
                .find(|field| field.key == "style")
            else {
                assert!(desc.styles.is_empty(), "{}", desc.id);
                continue;
            };
            let crate::schema::FieldKind::Select { options } = &field.kind else {
                panic!("style is not a select on {}", desc.id);
            };
            assert_eq!(options.len(), desc.styles.len(), "{}", desc.id);
            for uuid in desc.styles {
                assert!(
                    options.contains(&style_label(uuid).to_string()),
                    "{}",
                    desc.id
                );
            }
            assert_eq!(
                field.default.as_ref().and_then(Value::as_str),
                Some(desc.default_style),
                "{}",
                desc.id
            );
        }
    }

    #[test]
    fn reference_steps_inline_the_base64_of_every_image() {
        let model = provider("leonardo-phoenix-v1.0");
        let steps = model.reference_steps(&[encoded(10, 10), encoded(20, 20)]);
        assert_eq!(steps.len(), 2);
        for (index, step) in steps.into_iter().enumerate() {
            let crate::providers::ReferenceStep::Inline(crate::providers::ImageInput::RawBase64(
                text,
            )) = step
            else {
                panic!("expected an inline step");
            };
            let (width, height) = if index == 0 { (10, 10) } else { (20, 20) };
            assert_eq!(text, encoded(width, height).base64());
        }
    }

    #[test]
    fn parse_submit_builds_the_v1_poll_url() {
        let model = provider("leonardo-phoenix-v1.0");
        let submitted = model
            .parse_submit(&response(
                200,
                r#"{"generationId": "11111111-2222-4333-8444-555555555555"}"#,
            ))
            .unwrap();
        assert_eq!(
            submitted.poll_url.as_str(),
            "https://cloud.leonardo.ai/api/rest/v1/generations/11111111-2222-4333-8444-555555555555"
        );
        assert!(submitted.quote.is_none());

        let nested = model
            .parse_submit(&response(
                200,
                r#"{"generate": {"generationId": "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"}}"#,
            ))
            .unwrap();
        assert_eq!(
            nested.job_id.as_str(),
            "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee"
        );

        let sd = model
            .parse_submit(&response(
                200,
                r#"{"sdGenerationJob": {"generationId": "aaaaaaaa-bbbb-4ccc-8ddd-ffffffffffff"}}"#,
            ))
            .unwrap();
        assert_eq!(sd.job_id.as_str(), "aaaaaaaa-bbbb-4ccc-8ddd-ffffffffffff");

        let missing = model.parse_submit(&response(200, r#"{"apiCreditCost": 1}"#));
        assert!(matches!(missing, Err(ProviderError::Protocol(_))));
        let invalid = model.parse_submit(&response(200, r#"{"generationId": "../evil"}"#));
        assert!(matches!(invalid, Err(ProviderError::Protocol(_))));
    }

    #[test]
    fn poll_request_carries_bearer_auth() {
        let model = provider("leonardo-phoenix-v1.0");
        let submitted = model
            .parse_submit(&response(
                200,
                r#"{"generationId": "11111111-2222-4333-8444-555555555555"}"#,
            ))
            .unwrap();
        let poll = model.poll_request(&submitted);
        assert_eq!(poll.method, crate::http::Method::Get);
        assert_eq!(poll.auth, Some(ProviderId::Leonardo));
        assert!(matches!(poll.body, crate::http::Body::Empty));
    }

    #[test]
    fn parse_poll_maps_each_status() {
        let model = provider("leonardo-phoenix-v1.0");
        for status in ["PENDING", "QUEUED", "PROCESSING"] {
            let body = format!(
                r#"{{"generations_by_pk": {{"status": "{status}", "generated_images": []}}}}"#
            );
            match model.parse_poll(&response(200, &body)) {
                PollOutcome::InFlight {
                    status: got,
                    progress,
                } => {
                    assert_eq!(got, status);
                    assert_eq!(progress, None);
                }
                other => panic!("{status}: {other:?}"),
            }
        }
        let ready = model.parse_poll(&response(
            200,
            r#"{"generations_by_pk": {"status": "COMPLETE", "generated_images": [{"url": "https://cdn.leonardo.ai/users/u/generations/g/out.jpg", "nsfw": false}]}}"#,
        ));
        match ready {
            PollOutcome::Ready { url, .. } => {
                assert_eq!(
                    url.as_str(),
                    "https://cdn.leonardo.ai/users/u/generations/g/out.jpg"
                );
            }
            other => panic!("{other:?}"),
        }
        let nsfw = model.parse_poll(&response(
            200,
            r#"{"generations_by_pk": {"status": "COMPLETE", "generated_images": [{"url": "https://cdn.leonardo.ai/u/out.jpg", "nsfw": true}]}}"#,
        ));
        assert!(matches!(nsfw, PollOutcome::Failed(ProviderError::Blocked)));
        let empty = model.parse_poll(&response(
            200,
            r#"{"generations_by_pk": {"status": "COMPLETE", "generated_images": []}}"#,
        ));
        assert!(matches!(empty, PollOutcome::Failed(ProviderError::NoImage)));
        let evil = model.parse_poll(&response(
            200,
            r#"{"generations_by_pk": {"status": "COMPLETE", "generated_images": [{"url": "https://evil.example.com/a.png"}]}}"#,
        ));
        assert!(matches!(
            evil,
            PollOutcome::Failed(ProviderError::Protocol(text)) if text.contains("host")
        ));
        let failed = model.parse_poll(&response(
            200,
            r#"{"generations_by_pk": {"status": "FAILED"}}"#,
        ));
        assert!(matches!(
            failed,
            PollOutcome::Failed(ProviderError::Protocol(_))
        ));
        let missing = model.parse_poll(&response(200, r#"{"generations_by_pk": null}"#));
        assert!(
            matches!(missing, PollOutcome::Failed(ProviderError::Protocol(text)) if text.contains("generations_by_pk"))
        );
    }

    #[test]
    fn non_200_responses_classify_per_the_failure_table() {
        let model = provider("leonardo-phoenix-v1.0");
        assert!(matches!(
            model.parse_submit(&response(401, "no")),
            Err(ProviderError::Auth)
        ));
        assert!(matches!(
            model.parse_submit(&response(403, "no")),
            Err(ProviderError::Auth)
        ));
        assert!(matches!(
            model.parse_submit(&response(402, "no")),
            Err(ProviderError::OutOfCredits { .. })
        ));
        assert!(matches!(
            model.parse_submit(&response(404, "no")),
            Err(ProviderError::ModelNotFound)
        ));
        assert!(matches!(
            model.parse_poll(&response(404, "no")),
            PollOutcome::Failed(ProviderError::Protocol(_))
        ));
        assert!(matches!(
            model.parse_submit(&response(429, "no")),
            Err(ProviderError::RateLimited { retry_after: None })
        ));
        assert!(matches!(
            model.parse_submit(&response(
                400,
                r#"{"error": "content moderation filter: nude", "code": "unexpected"}"#
            )),
            Err(ProviderError::Blocked)
        ));
        assert!(matches!(
            model.parse_submit(&response(422, r#"{"error": "nsfw detected"}"#)),
            Err(ProviderError::Blocked)
        ));
        match model.parse_submit(&response(
            400,
            r#"{"error": "Unexpected variable foo", "code": "bad-request"}"#,
        )) {
            Err(ProviderError::Validation { message, .. }) => {
                assert_eq!(message, "Unexpected variable foo");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            model.parse_submit(&response(500, "boom")),
            Err(ProviderError::Http { status: 500, .. })
        ));
    }

    #[test]
    fn leonardo_poll_policy_uses_two_second_backoff() {
        let model = provider("leonardo-phoenix-v1.0");
        let policy = model.poll_policy();
        assert_eq!(policy.initial, Duration::from_secs(2));
        assert_eq!(policy.next_delay(1), Duration::from_secs(2).mul_f64(1.25));
        assert_eq!(policy.cap, Duration::from_secs(5));
        assert_eq!(policy.deadline, Duration::from_secs(600));
    }

    #[test]
    fn guidance_caps_match_the_model_table() {
        for desc in MODELS.iter() {
            let model = provider(desc.id);
            let max = model.spec().capabilities.image_slots.max();
            match desc.guidance {
                GuidanceMode::None => assert_eq!(max, 0, "{}", desc.id),
                GuidanceMode::MultiSlot { max_images, .. } => {
                    assert_eq!(max, max_images, "{}", desc.id)
                }
                GuidanceMode::KreaStyle => assert_eq!(max, 10, "{}", desc.id),
                GuidanceMode::ImageReference { max_images } => {
                    assert_eq!(max, max_images, "{}", desc.id)
                }
                GuidanceMode::RemoveBg => assert_eq!(max, 2, "{}", desc.id),
            }
            assert!(!model.spec().capabilities.supports_boxes);
            assert!(!model.spec().beta);
        }
    }

    #[test]
    fn all_undefined_style_lists_match_the_openapi_enums() {
        let counts: Vec<(&str, usize)> = MODELS
            .iter()
            .filter(|desc| !desc.styles.is_empty())
            .map(|desc| (desc.id, desc.styles.len()))
            .collect();
        assert_eq!(counts.len(), 30);
        for (id, count) in counts {
            let expected = match id {
                "leonardo-phoenix-v1.0" | "leonardo-phoenix-v0.9" => 23,
                "leonardo-lucid-origin" | "leonardo-ideogram-v3.0" => 8,
                "leonardo-lucid-realism" => 21,
                "leonardo-kino-xl" => 21,
                "leonardo-anime-xl" => 12,
                "leonardo-portrait-perfect" => 8,
                "leonardo-stock-photography" => 2,
                "leonardo-illustrative-albedo" => 16,
                "leonardo-concept-art" => 8,
                "leonardo-lifelike-vision" => 20,
                "leonardo-graphic-design" => 7,
                "leonardo-lightning-xl" => 22,
                "leonardo-flux-dev" | "leonardo-flux-schnell" => 16,
                "leonardo-flux-kontext-pro"
                | "leonardo-flux-kontext-max"
                | "leonardo-flux-pro-2.0"
                | "leonardo-gemini-2.5-flash-image"
                | "leonardo-nano-banana-2"
                | "leonardo-nano-banana-2.1"
                | "leonardo-nano-banana-2-lite"
                | "leonardo-gemini-image-2" => 19,
                "leonardo-seedream-4.0" | "leonardo-seedream-4.5" => 8,
                "leonardo-gpt-image-1.5"
                | "leonardo-gpt-image-2"
                | "leonardo-gpt-image-2.5-flare"
                | "leonardo-gpt-image-2.5-sunburst" => 21,
                other => panic!("unexpected style model {other}"),
            };
            assert_eq!(count, expected, "{id}");
        }
    }

    #[test]
    fn seed_is_omitted_when_zero_or_absent() {
        let model = provider("leonardo-lucid-origin");
        let mut params = values(&model);
        params.insert("seed".into(), json!(0));
        let req = request(&model, "a fox", params, Vec::new());
        assert!(parameters(&model, &req).get("seed").is_none());

        let mut params = values(&model);
        params.insert("seed".into(), json!(42));
        let req = request(&model, "a fox", params, Vec::new());
        assert_eq!(parameters(&model, &req)["seed"], json!(42));
    }

    #[test]
    fn excess_reference_images_are_omitted_not_rejected() {
        let model = provider("leonardo-flux-kontext-pro");
        let params = values(&model);
        let req = request(&model, "a fox", params, vec![encoded(1, 1); 6]);
        let params_out = parameters(&model, &req);
        let entries = params_out["guidances"]["image_reference"]
            .as_array()
            .unwrap();
        assert_eq!(entries.len(), 4);
    }

    #[test]
    fn image_slots_carry_the_leonardo_keys() {
        for desc in MODELS.iter() {
            let model = provider(desc.id);
            let crate::model_spec::ImageSlots::List { key, .. } =
                &model.spec().capabilities.image_slots
            else {
                assert!(matches!(desc.guidance, GuidanceMode::None), "{}", desc.id);
                continue;
            };
            let expected = match desc.guidance {
                GuidanceMode::MultiSlot { .. } => "guidances",
                GuidanceMode::KreaStyle => "style",
                GuidanceMode::ImageReference { .. } | GuidanceMode::RemoveBg => "image_reference",
                GuidanceMode::None => unreachable!("None slots have no list"),
            };
            assert_eq!(key, expected, "{}", desc.id);
        }
    }
}
