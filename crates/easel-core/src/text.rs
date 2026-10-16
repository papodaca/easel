#[must_use]
pub fn sanitize_for_display(text: &str, max_len: usize) -> String {
    let mut cleaned = String::new();
    for character in text.chars() {
        let keep = character == '\n'
            || character == '\t'
            || (!character.is_control() && !is_bidi(character));
        if keep {
            cleaned.push(character);
        }
        if cleaned.len() >= max_len {
            break;
        }
    }
    if cleaned.len() > max_len {
        cleaned.truncate(max_len);
    }
    cleaned
}

fn is_bidi(character: char) -> bool {
    matches!(character, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

#[cfg(test)]
mod tests {
    #[test]
    fn sanitize_strips_controls_and_truncates() {
        let mut text = "a\u{202E}b\u{0001}c\n\t".to_string();
        text.push_str(&"x".repeat(20_000));
        let strip = super::sanitize_for_display(&text, 2 * 1024);
        let details = super::sanitize_for_display(&text, 16 * 1024);
        assert!(strip.len() <= 2 * 1024);
        assert!(details.len() <= 16 * 1024);
        assert!(!strip.contains('\u{202E}'));
        assert!(!strip.contains('\u{0001}'));
        assert!(strip.contains('\n') && strip.contains('\t'));
    }
}
