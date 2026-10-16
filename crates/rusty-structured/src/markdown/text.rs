use std::borrow::Cow;

const TAB: &str = "    ";

pub(super) fn normalise_line_ends(source: &str) -> Cow<'_, str> {
    if !source.contains('\r') {
        return Cow::Borrowed(source);
    }
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find('\r') {
        out.push_str(&rest[..at]);
        out.push('\n');
        rest = &rest[at + 1..];
        rest = rest.strip_prefix('\n').unwrap_or(rest);
    }
    out.push_str(rest);
    Cow::Owned(out)
}

fn is_unsafe(ch: char) -> bool {
    ch.is_control() && ch != '\n'
}

pub(super) fn clean(text: &str) -> Cow<'_, str> {
    if !text.chars().any(is_unsafe) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + TAB.len());
    for ch in text.chars() {
        match ch {
            '\t' => out.push_str(TAB),
            ch if is_unsafe(ch) => out.push(' '),
            ch => out.push(ch),
        }
    }
    Cow::Owned(out)
}

fn floor_boundary(text: &str, at: usize) -> usize {
    (0..=at.min(text.len()))
        .rev()
        .find(|at| text.is_char_boundary(*at))
        .unwrap_or(0)
}

pub(super) fn cut_point(text: &str, room: usize, prefer: char) -> usize {
    let limit = floor_boundary(text, room);
    let head = &text[..limit];
    match head.rfind(prefer) {
        Some(at) if at > 0 => at + prefer.len_utf8(),
        _ => limit,
    }
}

pub(super) fn prefix(text: &str, room: usize) -> &str {
    &text[..floor_boundary(text, room)]
}

#[cfg(test)]
mod tests {
    use super::{clean, cut_point, normalise_line_ends};

    #[test]
    fn line_ends_become_newlines() {
        assert_eq!(normalise_line_ends("a\r\nb\rc\n"), "a\nb\nc\n");
        assert_eq!(normalise_line_ends("\r\r\n"), "\n\n");
    }

    #[test]
    fn cuts_prefer_the_given_character_and_respect_char_boundaries() {
        assert_eq!(cut_point("ab cd ef", 6, ' '), 6);
        assert_eq!(cut_point("abcdef", 4, ' '), 4);
        assert_eq!(cut_point("ééé", 3, ' '), 2);
        assert_eq!(clean("a\tb\u{7}"), "a    b ");
    }
}
