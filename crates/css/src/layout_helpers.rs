//! Text layout helper utilities.

#[inline]
pub fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_ws = false;
    for character in text.chars() {
        if character.is_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            in_ws = false;
            out.push(character);
        }
    }
    out.trim().to_owned()
}

#[inline]
pub fn reorder_bidi_for_display(text: &str) -> String {
    text.to_owned()
}
