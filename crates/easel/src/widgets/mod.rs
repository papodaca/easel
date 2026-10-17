#![allow(dead_code, unused_imports)]

mod checkbox;
mod number_input;
mod select;
mod slider;
mod text_area;
mod text_input;

pub use checkbox::Checkbox;
pub use number_input::NumberInput;
pub use select::Select;
pub use slider::Slider;
pub use text_area::TextArea;
pub use text_input::TextInput;

pub fn bind_keys(cx: &mut gpui::App) {
    text_input::bind_keys(cx);
    text_area::bind_keys(cx);
    select::bind_keys(cx);
}

pub fn field_row(
    label: Option<&str>,
    help: Option<&str>,
    control: impl gpui::IntoElement,
) -> gpui::Div {
    use gpui::{ParentElement, Styled, px, rgb};

    use crate::theme;

    let mut column = gpui::div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .w_full()
        .min_w(px(0.0))
        .text_color(rgb(theme::TEXT));
    if let Some(label) = label {
        column = column.child(
            gpui::div()
                .text_xs()
                .text_color(rgb(theme::TEXT_MUTED))
                .child(label.to_owned()),
        );
    }
    column = column.child(control);
    if let Some(help) = help {
        column = column.child(
            gpui::div()
                .text_xs()
                .text_color(rgb(theme::TEXT_MUTED))
                .child(help.to_owned()),
        );
    }
    column
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use gpui::{
        ClipboardItem, Focusable as _, Modifiers, MouseButton, TestAppContext, VisualTestContext,
        point, prelude::*, px,
    };
    use serde_json::{Value, json};

    use super::{Checkbox, NumberInput, Select, Slider, TextArea, TextInput};

    fn open<T: gpui::Render + gpui::Focusable>(
        cx: &mut TestAppContext,
        build: impl FnOnce(&mut gpui::Window, &mut gpui::Context<T>) -> T + 'static,
    ) -> (gpui::WindowHandle<T>, VisualTestContext) {
        let window = cx.update(|cx| {
            super::bind_keys(cx);
            cx.open_window(gpui::WindowOptions::default(), |window, cx| {
                let entity = cx.new(|cx| build(window, cx));
                entity.focus_handle(cx).focus(window, cx);
                entity
            })
            .unwrap()
        });
        let visual = VisualTestContext::from_window(window.into(), cx);
        (window, visual)
    }

    fn changes() -> Arc<Mutex<Vec<Value>>> {
        Arc::new(Mutex::new(Vec::new()))
    }

    #[gpui::test]
    fn text_input_inserts_moves_and_clears(cx: &mut TestAppContext) {
        let (window, mut cx) = open(cx, |_, cx| TextInput::new(cx));
        let input = window.root(&mut cx).unwrap();
        cx.simulate_input("abc");
        cx.simulate_keystrokes("left");
        cx.simulate_input("x");
        assert_eq!(
            input.read_with(&cx, |input, _| input.text().to_owned()),
            "abxc"
        );
        cx.simulate_keystrokes("ctrl-a backspace");
        assert_eq!(input.read_with(&cx, |input, _| input.text().to_owned()), "");
    }

    #[gpui::test]
    fn text_input_paste_inserts_clipboard_text(cx: &mut TestAppContext) {
        let (window, mut cx) = open(cx, |_, cx| TextInput::new(cx));
        let input = window.root(&mut cx).unwrap();
        cx.update(|_, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string("yz".into()));
        });
        cx.simulate_input("a");
        cx.simulate_keystrokes("ctrl-v");
        assert_eq!(
            input.read_with(&cx, |input, _| input.text().to_owned()),
            "ayz"
        );
    }

    #[gpui::test]
    fn text_area_newline_and_vertical_motion(cx: &mut TestAppContext) {
        let (window, mut cx) = open(cx, |_, cx| TextArea::new(cx));
        let area = window.root(&mut cx).unwrap();
        cx.simulate_input("ab");
        cx.simulate_keystrokes("enter");
        cx.simulate_input("cd");
        assert_eq!(
            area.read_with(&cx, |area, _| area.text().to_owned()),
            "ab\ncd"
        );
        cx.simulate_keystrokes("up");
        cx.simulate_input("X");
        assert_eq!(
            area.read_with(&cx, |area, _| area.text().to_owned()),
            "abX\ncd"
        );
        cx.simulate_keystrokes("down");
        cx.simulate_input("Y");
        assert_eq!(
            area.read_with(&cx, |area, _| area.text().to_owned()),
            "abX\ncdY"
        );
    }

    #[gpui::test]
    fn number_input_clamps_reverts_and_steps(cx: &mut TestAppContext) {
        let seen = changes();
        let record = Arc::clone(&seen);
        let (window, mut cx) = open(cx, move |_, cx| {
            let mut input = NumberInput::new(0.0, 7.0, 1.0, cx);
            let record = Arc::clone(&record);
            input.on_change(
                Box::new(move |value, _, _| record.lock().expect("changes").push(value.clone())),
                cx,
            );
            input
        });
        let input = window.root(&mut cx).unwrap();
        cx.simulate_input("9");
        cx.update(|window, cx| {
            window.blur(cx);
            window.draw(cx).clear(cx);
        });
        assert_eq!(
            input.read_with(&cx, |input, _| input.value().as_f64()),
            Some(7.0)
        );
        assert_eq!(seen.lock().expect("changes").as_slice(), &[json!(7.0)]);

        let before = seen.lock().expect("changes").len();
        cx.update(|window, cx| input.focus_handle(cx).focus(window, cx));
        cx.simulate_input("abc");
        cx.update(|window, cx| {
            window.blur(cx);
            window.draw(cx).clear(cx);
        });
        assert_eq!(
            input.read_with(&cx, |input, _| input.value().as_f64()),
            Some(7.0)
        );
        assert_eq!(seen.lock().expect("changes").len(), before);

        cx.update(|window, cx| input.focus_handle(cx).focus(window, cx));
        let point = input.read_with(&cx, |input, _| input.step_up_point());
        cx.simulate_click(point, Modifiers::default());
        assert_eq!(
            input.read_with(&cx, |input, _| input.value().as_f64()),
            Some(7.0)
        );
    }

    #[gpui::test]
    fn select_keyboard_chooses_the_third_option(cx: &mut TestAppContext) {
        let seen = changes();
        let record = Arc::clone(&seen);
        let (window, mut cx) = open(cx, move |_, cx| {
            let mut select = Select::new(vec!["red".into(), "green".into(), "blue".into()], cx);
            let record = Arc::clone(&record);
            select.on_change(
                Box::new(move |value, _, _| record.lock().expect("changes").push(value.clone())),
                cx,
            );
            select
        });
        let select = window.root(&mut cx).unwrap();
        cx.simulate_keystrokes("enter down down enter");
        assert_eq!(
            select.read_with(&cx, |select, _| select.value().clone()),
            json!("blue")
        );
        assert_eq!(seen.lock().expect("changes").as_slice(), &[json!("blue")]);

        let before = seen.lock().expect("changes").len();
        cx.simulate_keystrokes("enter down escape");
        assert_eq!(
            select.read_with(&cx, |select, _| select.value().clone()),
            json!("blue")
        );
        assert_eq!(seen.lock().expect("changes").len(), before);
    }

    #[gpui::test]
    fn select_click_opens_the_menu(cx: &mut TestAppContext) {
        let (window, mut cx) = open(cx, |_, cx| {
            Select::new(vec!["red".into(), "green".into(), "blue".into()], cx)
        });
        let select = window.root(&mut cx).unwrap();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let point = select.read_with(&cx, |select, _| select.click_point());
        cx.simulate_click(point, Modifiers::default());
        assert!(select.read_with(&cx, |select, _| select.is_open()));
        cx.simulate_keystrokes("down down enter");
        assert_eq!(
            select.read_with(&cx, |select, _| select.value().clone()),
            json!("blue")
        );
    }

    #[gpui::test]
    fn releasing_over_a_snapped_menu_does_not_choose(cx: &mut TestAppContext) {
        let (window, mut cx) = open(cx, |_, cx| {
            Select::new(vec!["red".into(), "green".into(), "blue".into()], cx)
        });
        let select = window.root(&mut cx).unwrap();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let point = select.read_with(&cx, |select, _| select.click_point());
        cx.simulate_mouse_down(point, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        assert!(select.read_with(&cx, |select, _| select.is_open()));
        let option = select.read_with(&cx, |select, _| select.menu_point(0));
        cx.simulate_mouse_up(option, MouseButton::Left, Modifiers::default());
        assert!(select.read_with(&cx, |select, _| select.is_open()));
        assert_eq!(
            select.read_with(&cx, |select, _| select.value().clone()),
            serde_json::Value::Null
        );
    }

    #[gpui::test]
    fn select_click_again_closes_without_changing_the_value(cx: &mut TestAppContext) {
        let seen = changes();
        let record = Arc::clone(&seen);
        let (window, mut cx) = open(cx, move |_, cx| {
            let mut select = Select::new(vec!["red".into(), "green".into(), "blue".into()], cx);
            select.set_value(json!("green"), cx);
            let record = Arc::clone(&record);
            select.on_change(
                Box::new(move |value, _, _| record.lock().expect("changes").push(value.clone())),
                cx,
            );
            select
        });
        let select = window.root(&mut cx).unwrap();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let point = select.read_with(&cx, |select, _| select.click_point());
        cx.simulate_click(point, Modifiers::default());
        assert!(select.read_with(&cx, |select, _| select.is_open()));
        cx.simulate_click(point, Modifiers::default());
        assert!(!select.read_with(&cx, |select, _| select.is_open()));
        assert_eq!(
            select.read_with(&cx, |select, _| select.value().clone()),
            json!("green")
        );
        assert!(seen.lock().expect("changes").is_empty());
    }

    #[gpui::test]
    fn checkbox_toggles_on_click_and_space(cx: &mut TestAppContext) {
        let seen = changes();
        let record = Arc::clone(&seen);
        let (window, mut cx) = open(cx, move |_, cx| {
            let mut checkbox = Checkbox::new(cx);
            let record = Arc::clone(&record);
            checkbox.on_change(
                Box::new(move |value, _, _| record.lock().expect("changes").push(value.clone())),
                cx,
            );
            checkbox
        });
        let checkbox = window.root(&mut cx).unwrap();
        let point = checkbox.read_with(&cx, |checkbox, _| checkbox.click_point());
        cx.simulate_click(point, Modifiers::default());
        assert_eq!(
            checkbox.read_with(&cx, |checkbox, _| checkbox.value().as_bool()),
            Some(true)
        );
        cx.simulate_keystrokes("space");
        assert_eq!(
            checkbox.read_with(&cx, |checkbox, _| checkbox.value().as_bool()),
            Some(false)
        );
        assert_eq!(
            seen.lock().expect("changes").as_slice(),
            &[json!(true), json!(false)]
        );
    }

    #[gpui::test]
    fn slider_maps_track_position_and_drag(cx: &mut TestAppContext) {
        let seen = changes();
        let record = Arc::clone(&seen);
        let (window, mut cx) = open(cx, move |_, cx| {
            let mut slider = Slider::new(0.0, 100.0, cx);
            let record = Arc::clone(&record);
            slider.on_change(
                Box::new(move |value, _, _| record.lock().expect("changes").push(value.clone())),
                cx,
            );
            slider
        });
        let slider = window.root(&mut cx).unwrap();
        let bounds = slider.read_with(&cx, |slider, _| slider.track_bounds());
        let y = bounds.origin.y + bounds.size.height / 2.0;
        let at_quarter = point(bounds.origin.x + bounds.size.width * 0.25, y);
        cx.simulate_mouse_down(at_quarter, MouseButton::Left, Modifiers::default());
        assert_eq!(
            slider.read_with(&cx, |slider, _| slider.value().as_f64()),
            Some(25.0)
        );
        let at_end = point(bounds.origin.x + bounds.size.width - px(1.0), y);
        cx.simulate_mouse_move(at_end, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(at_end, MouseButton::Left, Modifiers::default());
        assert_eq!(
            slider.read_with(&cx, |slider, _| slider.value().as_f64()),
            Some(100.0)
        );
        let values = seen.lock().expect("changes").clone();
        assert!(
            values.first().and_then(Value::as_f64) == Some(25.0),
            "{values:?}"
        );
        assert!(
            values.last().and_then(Value::as_f64) == Some(100.0),
            "{values:?}"
        );
    }
}
