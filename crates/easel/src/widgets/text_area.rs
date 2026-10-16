#![allow(dead_code)]

use gpui::{App, Context, KeyBinding};

use super::text_input::{Down, Newline, TextInput, Up};

pub struct TextArea;

impl TextArea {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(cx: &mut Context<TextInput>) -> TextInput {
        TextInput::multiline(cx)
    }
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", Up, Some("TextArea")),
        KeyBinding::new("down", Down, Some("TextArea")),
        KeyBinding::new("enter", Newline, Some("TextArea")),
    ]);
}
