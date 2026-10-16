//! One dark palette. Regions use different backgrounds so an unstyled div cannot hide them.

pub const WINDOW: u32 = 0x12141c;
pub const TOOLBAR: u32 = 0x2e3a4f;
pub const CANVAS: u32 = 0x1b1e27;
pub const SIDE_PANEL: u32 = 0x3c314c;
pub const STRIP: u32 = 0x243028;
#[cfg_attr(not(test), allow(dead_code))]
pub const FIELD: u32 = 0x232838;
pub const BUTTON: u32 = 0x4c5b78;
pub const BUTTON_HOVER: u32 = 0x5d6e90;
pub const TEXT: u32 = 0xe7e5e4;
pub const MUTED: u32 = 0xa8a29e;
pub const TOAST_INFO: u32 = 0x1d3d34;
pub const TOAST_ERROR: u32 = 0x4a2430;

pub const SIDE_PANEL_WIDTH: f32 = 320.0;
pub const TOOLBAR_HEIGHT: f32 = 72.0;
pub const STRIP_HEIGHT: f32 = 48.0;
