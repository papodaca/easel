use gpui::{App, KeyBinding, actions};

actions!(
    easel,
    [Open, Paste, SaveAs, Fit, ActualSize, Settings, Quit]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-o", Open, None),
        KeyBinding::new("ctrl-v", Paste, None),
        KeyBinding::new("ctrl-shift-s", SaveAs, None),
        KeyBinding::new("ctrl-0", Fit, None),
        KeyBinding::new("ctrl-1", ActualSize, None),
        KeyBinding::new("ctrl-,", Settings, None),
        KeyBinding::new("ctrl-q", Quit, None),
    ]);
}
