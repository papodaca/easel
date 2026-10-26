# AGENTS.md

Rules for working on easel, a gpui desktop app pinned to Zed `v1.25.3`.

## Look and feel

The window copies Zed's shape: a drawn title bar, 1 px borders between panes, three background levels, small flat buttons. Keep it that way.

- Colors come from `crates/easel/src/theme.rs` only. No `rgb(0x...)` literals in views or widgets. Add a named constant if a color is missing.
- Backgrounds are a ladder: `TITLE_BAR` for the title and status bars, `PANEL` for docks and the side panel, `EDITOR` for the canvas, `SURFACE` for fields and popovers. Do not invent a fourth level or tint a region to make it visible.
- Separate regions with `border_*_1().border_color(rgb(theme::BORDER))`, never with contrasting fills.
- Pane headers and the toolbar are `theme::PANE_HEADER_HEIGHT` (32 px) so their bottom borders line up across the split. Use `ui::pane_header` for new panes.
- Buttons go through `ui::Button::ghost` (toolbar, headers, secondary actions) or `ui::Button::filled` (primary form actions). Do not build a button out of a bare `div().bg(...)`.
- Text defaults to 14 px (`text_sm`) from the root. Labels, help text, and the status bar use `text_xs`. Muted text uses `theme::TEXT_MUTED`; errors use `theme::ERROR`.
- The UI font is IBM Plex Sans, bundled in `crates/easel/assets/fonts` under the OFL and loaded by `assets::load_fonts`. Do not depend on a system font.
- Toasts are bottom-right cards (`notifications.rs`). Status text belongs in the status bar, not the title bar.

## Window chrome

- `chrome::client_side_decorations` wraps the root; `chrome::TitleBar` handles move, double-click maximize, right-click menu, and window controls. Both come from the gpui `window_shadow.rs` example (Apache-2.0). Zed's `title_bar`, `platform_title_bar`, `ui`, and `workspace` crates are GPL. Read them for ideas, never copy from them.
- Window controls follow `cx.button_layout()` and fall back to minimize/maximize/close on the right. Icons are the hand-drawn SVGs in `crates/easel/assets/icons`, served by `assets::Assets`.
- Views that touch the window corners (title bar, status bar) call `chrome::round_client_corners` with the current `chrome::tiling(window)` so corners stay square when tiled.

## Verification

- Before claiming UI work is done: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run -p easel`.
- Visual checks: run `WAYLAND_DISPLAY='' cargo run -p easel` and capture the window with `magick import -window <id>` (find the id with `xwininfo -root -tree | rg easel`). Do not inject clicks or keys with `xdotool`; pointer coordinates do not line up on this desktop and a stray click lands on the close button. Ask the user to drive the app and report instead.
- Widget tests read element bounds from the entity (`track_bounds`, `click_point`), so restyling a widget's size is safe as long as those helpers stay accurate.

## Conventions carried over from the plans

- Linux only, `ctrl-` bindings, no `cmd-`.
- No third-party widget crates. Widgets live in `crates/easel/src/widgets` and share `widgets::field_row` for label and help text.
- Nothing from Zed outside `gpui`, `gpui_platform`, and `gpui_tokio` enters `Cargo.toml`; `deny.toml` rejects GPL licenses.
