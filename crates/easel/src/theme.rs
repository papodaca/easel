//! One dark palette, sampled from the Zed window this app sits next to.
//!
//! Three background levels do the work Zed's theme does: the title and status
//! bars are darkest, docks and panels one step up, the editor (here, the canvas)
//! lightest. Everything else is a 1 px border or a text color.

use gpui::Pixels;

// Backgrounds, darkest to lightest.
pub const TITLE_BAR: u32 = 0x11111b;
pub const PANEL: u32 = 0x181825;
pub const SURFACE: u32 = 0x1e1e2e;
pub const EDITOR: u32 = 0x24273a;

// Lines.
pub const BORDER: u32 = 0x313244;
pub const BORDER_FOCUSED: u32 = 0x5b6078;

// Interactive element fills.
pub const ELEMENT: u32 = 0x313244;
pub const ELEMENT_HOVER: u32 = 0x45475a;
pub const ELEMENT_ACTIVE: u32 = 0x585b70;
pub const GHOST_HOVER: u32 = 0x252637;
pub const SELECTION: u32 = 0x3b3d58;

// Text.
pub const TEXT: u32 = 0xcdd6f4;
pub const TEXT_MUTED: u32 = 0xa6adc8;
pub const TEXT_DISABLED: u32 = 0x6c7086;
pub const ACCENT: u32 = 0x89b4fa;
pub const ERROR: u32 = 0xf38ba8;

// Box outlines. Fills use the same hue at `BOX_FILL_ALPHA`.
pub const BOX_FILL_ALPHA: f32 = 0.15;
pub const BOX_OUTLINE_WIDTH: f32 = 2.0;
pub const BOX_LAYOUT: u32 = 0x89b4fa;
pub const BOX_NEW: u32 = 0xa6e3a1;
pub const BOX_KEEP: u32 = 0xf9e2af;
pub const BOX_MOVE: u32 = 0xcba6f7;
pub const BOX_REMOVE: u32 = 0xf38ba8;

// Fields keep the darkest surface so they read as "inset".
pub const FIELD: u32 = SURFACE;

// Window chrome. Match Zed's client-side decoration constants.
pub const TITLE_BAR_HEIGHT: f32 = 34.0;
pub const PANE_HEADER_HEIGHT: f32 = 32.0;
pub const STATUS_BAR_HEIGHT: f32 = 28.0;
pub const SIDE_PANEL_WIDTH: f32 = 320.0;
pub const STRIP_HEIGHT: f32 = 56.0;
pub const WINDOW_ROUNDING: Pixels = gpui::px(10.0);
pub const WINDOW_SHADOW: Pixels = gpui::px(10.0);
pub const WINDOW_BORDER: Pixels = gpui::px(1.0);

pub fn color(packed: u32, alpha: f32) -> gpui::Rgba {
    gpui::Rgba {
        r: ((packed >> 16) & 0xff) as f32 / 255.0,
        g: ((packed >> 8) & 0xff) as f32 / 255.0,
        b: (packed & 0xff) as f32 / 255.0,
        a: alpha,
    }
}
