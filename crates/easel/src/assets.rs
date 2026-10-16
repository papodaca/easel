//! Embedded assets: the UI font and the handful of SVG icons the chrome needs.
//!
//! IBM Plex Sans is the typeface Zed ships as "Zed Plex Sans". It is bundled
//! here under the SIL Open Font License (see `assets/fonts/ibm-plex-sans/license.txt`)
//! so the window looks the same on every machine instead of falling back to
//! whatever fontconfig picks.

use std::borrow::Cow;

use gpui::{App, AssetSource, SharedString};

pub const UI_FONT: &str = "IBM Plex Sans";

const FONTS: [&[u8]; 3] = [
    include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf"),
    include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-Italic.ttf"),
    include_bytes!("../assets/fonts/ibm-plex-sans/IBMPlexSans-SemiBold.ttf"),
];

const APP_ICON_SVG: &[u8] = include_bytes!("../assets/icons/app.svg");

const ICONS: [(&str, &[u8]); 6] = [
    ("icons/app.svg", APP_ICON_SVG),
    (
        "icons/minimize.svg",
        include_bytes!("../assets/icons/minimize.svg"),
    ),
    (
        "icons/maximize.svg",
        include_bytes!("../assets/icons/maximize.svg"),
    ),
    (
        "icons/restore.svg",
        include_bytes!("../assets/icons/restore.svg"),
    ),
    (
        "icons/close.svg",
        include_bytes!("../assets/icons/close.svg"),
    ),
    (
        "icons/chevron-left.svg",
        include_bytes!("../assets/icons/chevron-left.svg"),
    ),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}

const APP_ID: &str = "easel";

/// Rasterize `app.svg` for the X11 window icon. Wayland ignores this field and
/// uses the desktop entry instead.
pub fn window_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(APP_ICON_SVG, &options)
        .inspect_err(|error| log::warn!("app icon: {error}"))
        .ok()?;
    let size = tree.size();
    let width = size.width();
    let height = size.height();
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let target = 128.0;
    let scale = target / width.max(height);
    let pixel_w = (width * scale).round().max(1.0) as u32;
    let pixel_h = (height * scale).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pixel_w, pixel_h)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut pixels = pixmap.take();
    unpremultiply(&mut pixels);
    let image = image::RgbaImage::from_raw(pixel_w, pixel_h, pixels)?;
    Some(std::sync::Arc::new(image))
}

/// Put the icon and a desktop entry where GNOME looks them up for a Wayland
/// `app_id` of `easel`.
pub fn install_shell_icon() {
    let Some(data) = data_home() else {
        return;
    };
    let icon_path = data.join("icons/hicolor/scalable/apps/easel.svg");
    let desktop_path = data.join("applications/easel.desktop");
    if let Err(error) = write_if_changed(&icon_path, APP_ICON_SVG) {
        log::warn!("app icon: {error}");
        return;
    }
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=easel\nExec=easel\nIcon=easel\nTerminal=false\nCategories=Graphics;\nStartupWMClass={APP_ID}\n"
    );
    if let Err(error) = write_if_changed(&desktop_path, desktop.as_bytes()) {
        log::warn!("app icon: {error}");
    }
}

pub fn app_id() -> &'static str {
    APP_ID
}

fn data_home() -> Option<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        return Some(std::path::PathBuf::from(path));
    }
    std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".local/share"))
}

fn write_if_changed(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Ok(existing) = std::fs::read(path)
        && existing == bytes
    {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)
}

fn unpremultiply(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 {
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 0;
        } else if alpha < 255 {
            pixel[0] = ((u32::from(pixel[0]) * 255) / alpha) as u8;
            pixel[1] = ((u32::from(pixel[1]) * 255) / alpha) as u8;
            pixel[2] = ((u32::from(pixel[2]) * 255) / alpha) as u8;
        }
    }
}

pub fn load_fonts(cx: &App) {
    let fonts = FONTS.iter().map(|bytes| Cow::Borrowed(*bytes)).collect();
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        log::warn!("could not load the bundled UI font: {error:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_icon_rasterizes() {
        let icon = window_icon().expect("app.svg rasterizes");
        assert_eq!(icon.width(), 128);
        assert_eq!(icon.height(), 128);
        assert!(icon.pixels().any(|pixel| pixel.0[3] > 0));
    }
}
