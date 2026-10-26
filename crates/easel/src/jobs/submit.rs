use std::time::{SystemTime, UNIX_EPOCH};

use easel_core::image_prep::{PrepOptions, prepare_for_upload};
use easel_core::job::JobRequest;
use easel_core::model_spec::ProviderId;

use crate::app_state::AppState;

pub fn build_request(app: &AppState) -> Result<JobRequest, String> {
    let compiled = app.compiled();
    let tier = app
        .values
        .get("resolution")
        .and_then(|value| value.as_str())
        .unwrap_or("1k");
    let tier = if tier == "4K" { "4k" } else { tier };
    let mut options = PrepOptions::for_resolution(tier);
    // Leonardo accepts 128x128 inputs (Aurora upscalers) and omits excess
    // images, so never fail preparation on a small or unsent image.
    let limit = if app.spec().provider == ProviderId::Leonardo {
        options.min_side = 128;
        app.max_sent_images()
    } else {
        usize::MAX
    };
    let mut sources: Vec<&crate::image_load::DecodedImage> = Vec::new();
    if let Some(image) = &app.image {
        sources.push(image);
    }
    sources.extend(app.extras.iter().map(std::convert::AsRef::as_ref));
    let mut images = Vec::new();
    for image in sources.into_iter().take(limit) {
        images.push(
            prepare_for_upload(&image.rgba_for_export, options).map_err(|err| err.to_string())?,
        );
    }
    Ok(JobRequest {
        model_id: app.model_id.clone(),
        compiled_prompt: compiled.text,
        params: app.values.clone(),
        images,
        box_snapshot: app.scene.boxes.clone(),
        created_at: utc_now(),
    })
}

pub fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let time = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = time / 3600;
    let minute = (time % 3600) / 60;
    let second = time % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Howard Hinnant's civil-from-days, days since 1970-01-01.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}
